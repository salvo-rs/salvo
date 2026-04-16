//! native_tls module
use std::error::Error as StdError;
use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, Result as IoResult};
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use futures_util::stream::{BoxStream, StreamExt};
use http::uri::Scheme;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_native_tls::TlsStream;
use tokio_util::sync::CancellationToken;

use crate::conn::tcp::{DynTcpAcceptor, TcpCoupler, ToDynTcpAcceptor};
use crate::conn::{Accepted, Acceptor, HandshakeStream, Holding, IntoConfigStream, Listener};
use crate::fuse::ArcFuseFactory;
use crate::Error;

use super::Identity;

/// NativeTlsListener
pub struct NativeTlsListener<S, C, T, E> {
    config_stream: S,
    inner: T,
    _phantom: std::marker::PhantomData<(C, E)>,
}
impl<S, C, T: Debug, E> Debug for NativeTlsListener<S, C, T, E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTlsListener")
            .field("inner", &self.inner)
            .finish()
    }
}
impl<S, C, T, E> NativeTlsListener<S, C, T, E>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<Identity, Error = E> + Send + 'static,
    T: Listener + Send,
    E: StdError + Send,
{
    /// Create a new `NativeTlsListener`.
    #[inline]
    pub fn new(config_stream: S, inner: T) -> Self {
        Self {
            config_stream,
            inner,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<S, C, T, E> Listener for NativeTlsListener<S, C, T, E>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<Identity, Error = E> + Send + 'static,
    T: Listener + Send + 'static,
    T::Acceptor: Send + 'static,
    <T::Acceptor as Acceptor>::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    E: StdError + Send + 'static,
{
    type Acceptor = NativeTlsAcceptor<T::Acceptor>;

    async fn try_bind(self) -> crate::Result<Self::Acceptor> {
        let mut config_stream = self.config_stream.into_stream().boxed();
        let initial = config_stream
            .next()
            .await
            .ok_or_else(|| Error::other("native_tls: config stream ended before yielding an initial tls config"))?;
        let identity = initial
            .try_into()
            .map_err(|err| IoError::other(err.to_string()))?;
        let acceptor = tokio_native_tls::native_tls::TlsAcceptor::new(identity)
            .map_err(|err| IoError::other(err.to_string()))?;
        let current_acceptor = Arc::new(ArcSwapOption::from(Some(Arc::new(
            tokio_native_tls::TlsAcceptor::from(acceptor),
        ))));
        let inner = self.inner.try_bind().await?;
        let cancel_reload = CancellationToken::new();

        tracing::info!("tls config loaded");
        tokio::spawn(reload_configs(
            config_stream,
            Arc::clone(&current_acceptor),
            cancel_reload.clone(),
        ));

        Ok(NativeTlsAcceptor::new(inner, current_acceptor, cancel_reload))
    }
}

async fn reload_configs<C, E>(
    mut config_stream: BoxStream<'static, C>,
    current_acceptor: Arc<ArcSwapOption<tokio_native_tls::TlsAcceptor>>,
    cancel_reload: CancellationToken,
) where
    C: TryInto<Identity, Error = E> + Send + 'static,
    E: StdError + Send + 'static,
{
    loop {
        tokio::select! {
            _ = cancel_reload.cancelled() => break,
            next = config_stream.next() => {
                let Some(config) = next else {
                    break;
                };
                match config.try_into() {
                    Ok(identity) => match tokio_native_tls::native_tls::TlsAcceptor::new(identity) {
                        Ok(acceptor) => {
                            current_acceptor.store(Some(Arc::new(tokio_native_tls::TlsAcceptor::from(
                                acceptor,
                            ))));
                            tracing::info!("tls config changed");
                        }
                        Err(err) => {
                            tracing::error!(error = ?err, "native_tls: invalid tls config, keeping previous config");
                        }
                    },
                    Err(err) => {
                        tracing::error!(error = ?err, "native_tls: invalid tls config, keeping previous config");
                    }
                }
            }
        }
    }
}

/// NativeTlsAcceptor
pub struct NativeTlsAcceptor<T> {
    inner: T,
    holdings: Vec<Holding>,
    current_acceptor: Arc<ArcSwapOption<tokio_native_tls::TlsAcceptor>>,
    cancel_reload: CancellationToken,
}
impl<T: Debug> Debug for NativeTlsAcceptor<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTlsAcceptor")
            .field("inner", &self.inner)
            .finish()
    }
}
impl<T> NativeTlsAcceptor<T>
where
    T: Acceptor + Send + 'static,
    <T as Acceptor>::Stream: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Create a new `NativeTlsAcceptor`.
    pub fn new(
        inner: T,
        current_acceptor: Arc<ArcSwapOption<tokio_native_tls::TlsAcceptor>>,
        cancel_reload: CancellationToken,
    ) -> Self {
        let holdings = inner
            .holdings()
            .iter()
            .map(|h| {
                #[allow(unused_mut)]
                let mut versions = h.http_versions.clone();
                #[cfg(feature = "http1")]
                if !versions.contains(&crate::http::Version::HTTP_11) {
                    versions.push(crate::http::Version::HTTP_11);
                }
                #[cfg(feature = "http2")]
                if !versions.contains(&crate::http::Version::HTTP_2) {
                    versions.push(crate::http::Version::HTTP_2);
                }
                Holding {
                    local_addr: h.local_addr.clone(),
                    http_versions: versions,
                    http_scheme: Scheme::HTTPS,
                }
            })
            .collect();
        Self {
            inner,
            holdings,
            current_acceptor,
            cancel_reload,
        }
    }

    /// Get the inner `Acceptor`.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Convert this `NativeTlsAcceptor` into a boxed `DynTcpAcceptor`.
    pub fn into_boxed(self) -> Box<dyn DynTcpAcceptor> {
        Box::new(ToDynTcpAcceptor(self))
    }
}

impl<T> Drop for NativeTlsAcceptor<T> {
    fn drop(&mut self) {
        self.cancel_reload.cancel();
    }
}

impl<T> Acceptor for NativeTlsAcceptor<T>
where
    T: Acceptor + Send + 'static,
    <T as Acceptor>::Stream: AsyncRead + AsyncWrite + Unpin + Send,
{
    type Coupler = TcpCoupler<Self::Stream>;
    type Stream = HandshakeStream<TlsStream<T::Stream>>;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> IoResult<Accepted<Self::Coupler, Self::Stream>> {
        let Accepted {
            coupler: _,
            stream,
            fusewire,
            local_addr,
            remote_addr,
            ..
        } = self.inner.accept(fuse_factory).await?;
        let Some(tls_acceptor) = self.current_acceptor.load_full() else {
            return Err(IoError::other("native_tls: no active tls config"));
        };
        let conn = async move { tls_acceptor.accept(stream).await.map_err(IoError::other) };
        Ok(Accepted {
            coupler: TcpCoupler::new(),
            stream: HandshakeStream::new(conn, fusewire.clone()),
            fusewire,
            local_addr,
            remote_addr,
            http_scheme: Scheme::HTTPS,
        })
    }
}

