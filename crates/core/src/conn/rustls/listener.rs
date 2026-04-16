//! rustls module
use std::error::Error as StdError;
use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, Result as IoResult};
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use futures_util::stream::{BoxStream, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;
use tokio_rustls::server::TlsStream;

use crate::conn::tcp::{DynTcpAcceptor, TcpCoupler, ToDynTcpAcceptor};
use crate::conn::{Accepted, Acceptor, HandshakeStream, Holding, IntoConfigStream, Listener};
use crate::fuse::ArcFuseFactory;
use crate::http::uri::Scheme;
use crate::Error;

use super::ServerConfig;

/// A wrapper of `Listener` with rustls.
pub struct RustlsListener<S, C, T, E> {
    config_stream: S,
    inner: T,
    _phantom: std::marker::PhantomData<(C, E)>,
}

impl<S, C, T, E> Debug for RustlsListener<S, C, T, E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RustlsListener").finish()
    }
}

impl<S, C, T, E> RustlsListener<S, C, T, E>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<ServerConfig, Error = E> + Send + 'static,
    T: Listener + Send,
    E: StdError + Send,
{
    /// Create a new `RustlsListener`.
    #[inline]
    pub fn new(config_stream: S, inner: T) -> Self {
        Self {
            config_stream,
            inner,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<S, C, T, E> Listener for RustlsListener<S, C, T, E>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<ServerConfig, Error = E> + Send + 'static,
    T: Listener + Send + 'static,
    T::Acceptor: Send + 'static,
    <T::Acceptor as Acceptor>::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    E: StdError + Send + 'static,
{
    type Acceptor = RustlsAcceptor<T::Acceptor>;

    async fn try_bind(self) -> crate::Result<Self::Acceptor> {
        let mut config_stream = self.config_stream.into_stream().boxed();
        let initial = config_stream
            .next()
            .await
            .ok_or_else(|| Error::other("rustls: config stream ended before yielding an initial tls config"))?;
        let initial: ServerConfig = initial
            .try_into()
            .map_err(|err| IoError::other(err.to_string()))?;
        let current_acceptor = Arc::new(ArcSwapOption::from(Some(Arc::new(
            tokio_rustls::TlsAcceptor::from(Arc::new(initial)),
        ))));
        let inner = self.inner.try_bind().await?;
        let cancel_reload = CancellationToken::new();

        tracing::info!("tls config loaded");
        tokio::spawn(reload_configs(
            config_stream,
            Arc::clone(&current_acceptor),
            cancel_reload.clone(),
        ));

        Ok(RustlsAcceptor::new(inner, current_acceptor, cancel_reload))
    }
}

async fn reload_configs<C, E>(
    mut config_stream: BoxStream<'static, C>,
    current_acceptor: Arc<ArcSwapOption<tokio_rustls::TlsAcceptor>>,
    cancel_reload: CancellationToken,
) where
    C: TryInto<ServerConfig, Error = E> + Send + 'static,
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
                    Ok(config) => {
                        current_acceptor.store(Some(Arc::new(tokio_rustls::TlsAcceptor::from(Arc::new(
                            config,
                        )))));
                        tracing::info!("tls config changed");
                    }
                    Err(err) => {
                        tracing::error!(error = ?err, "rustls: invalid tls config, keeping previous config");
                    }
                }
            }
        }
    }
}

/// A wrapper of `Acceptor` with rustls.
pub struct RustlsAcceptor<T> {
    inner: T,
    holdings: Vec<Holding>,
    current_acceptor: Arc<ArcSwapOption<tokio_rustls::TlsAcceptor>>,
    cancel_reload: CancellationToken,
}

impl<T> Debug for RustlsAcceptor<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RustlsAcceptor").finish()
    }
}

impl<T> RustlsAcceptor<T>
where
    T: Acceptor + Send + 'static,
    T::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    /// Create a new `RustlsAcceptor`.
    pub fn new(
        inner: T,
        current_acceptor: Arc<ArcSwapOption<tokio_rustls::TlsAcceptor>>,
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

    /// Convert this `RustlsAcceptor` into a boxed `DynTcpAcceptor`.
    pub fn into_boxed(self) -> Box<dyn DynTcpAcceptor> {
        Box::new(ToDynTcpAcceptor(self))
    }
}

impl<T> Drop for RustlsAcceptor<T> {
    fn drop(&mut self) {
        self.cancel_reload.cancel();
    }
}

impl<T> Acceptor for RustlsAcceptor<T>
where
    T: Acceptor + Send + 'static,
    <T as Acceptor>::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Coupler = TcpCoupler<Self::Stream>;
    type Stream = HandshakeStream<TlsStream<T::Stream>>;

    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

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
            return Err(IoError::other("rustls: no active tls config"));
        };
        Ok(Accepted {
            coupler: TcpCoupler::new(),
            stream: HandshakeStream::new(tls_acceptor.accept(stream), fusewire.clone()),
            fusewire,
            local_addr,
            remote_addr,
            http_scheme: Scheme::HTTPS,
        })
    }
}

