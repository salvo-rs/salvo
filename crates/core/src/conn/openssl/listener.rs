//! openssl module
use std::error::Error as StdError;
use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, Result as IoResult};
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use futures_util::stream::{BoxStream, StreamExt};
use http::uri::Scheme;
use openssl::ssl::{Ssl, SslAcceptor};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_openssl::SslStream;
use tokio_util::sync::CancellationToken;

use super::SslAcceptorBuilder;

use crate::conn::tcp::{DynTcpAcceptor, TcpCoupler, ToDynTcpAcceptor};
use crate::conn::{Accepted, Acceptor, HandshakeStream, Holding, IntoConfigStream, Listener};
use crate::fuse::ArcFuseFactory;
use crate::Error;

/// OpensslListener
pub struct OpensslListener<S, C, T, E> {
    config_stream: S,
    inner: T,
    _phantom: std::marker::PhantomData<(C, E)>,
}
impl<S, C, T: Debug, E> Debug for OpensslListener<S, C, T, E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpensslListener")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<S, C, T, E> OpensslListener<S, C, T, E>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<SslAcceptorBuilder, Error = E> + Send + 'static,
    T: Listener + Send,
    E: StdError + Send,
{
    /// Create new OpensslListener with config stream.
    #[inline]
    pub fn new(config_stream: S, inner: T) -> Self {
        Self {
            config_stream,
            inner,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<S, C, T, E> Listener for OpensslListener<S, C, T, E>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<SslAcceptorBuilder, Error = E> + Send + 'static,
    T: Listener + Send + 'static,
    T::Acceptor: Send + 'static,
    <T::Acceptor as Acceptor>::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    E: StdError + Send + 'static,
{
    type Acceptor = OpensslAcceptor<T::Acceptor>;

    async fn try_bind(self) -> crate::Result<Self::Acceptor> {
        let mut config_stream = self.config_stream.into_stream().boxed();
        let initial = config_stream
            .next()
            .await
            .ok_or_else(|| Error::other("openssl: config stream ended before yielding an initial tls config"))?;
        let builder: SslAcceptorBuilder = initial
            .try_into()
            .map_err(|err| IoError::other(err.to_string()))?;
        let current_acceptor = Arc::new(ArcSwapOption::from(Some(Arc::new(builder.build()))));
        let inner = self.inner.try_bind().await?;
        let cancel_reload = CancellationToken::new();

        tracing::info!("tls config loaded");
        tokio::spawn(reload_configs(
            config_stream,
            Arc::clone(&current_acceptor),
            cancel_reload.clone(),
        ));

        Ok(OpensslAcceptor::new(inner, current_acceptor, cancel_reload))
    }
}

async fn reload_configs<C, E>(
    mut config_stream: BoxStream<'static, C>,
    current_acceptor: Arc<ArcSwapOption<SslAcceptor>>,
    cancel_reload: CancellationToken,
) where
    C: TryInto<SslAcceptorBuilder, Error = E> + Send + 'static,
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
                    Ok(builder) => {
                        current_acceptor.store(Some(Arc::new(builder.build())));
                        tracing::info!("tls config changed");
                    }
                    Err(err) => {
                        tracing::error!(error = ?err, "openssl: invalid tls config, keeping previous config");
                    }
                }
            }
        }
    }
}

/// OpensslAcceptor
pub struct OpensslAcceptor<T> {
    inner: T,
    holdings: Vec<Holding>,
    current_acceptor: Arc<ArcSwapOption<SslAcceptor>>,
    cancel_reload: CancellationToken,
}
impl<T> Debug for OpensslAcceptor<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpensslAcceptor")
            .field("holdings", &self.holdings)
            .finish()
    }
}
impl<T> OpensslAcceptor<T>
where
    T: Acceptor + Send + 'static,
    T::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    /// Create new OpensslAcceptor.
    pub fn new(
        inner: T,
        current_acceptor: Arc<ArcSwapOption<SslAcceptor>>,
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

    /// Convert this `OpoensslAcceptor` into a boxed `DynTcpAcceptor`.
    pub fn into_boxed(self) -> Box<dyn DynTcpAcceptor> {
        Box::new(ToDynTcpAcceptor(self))
    }
}

impl<T> Drop for OpensslAcceptor<T> {
    fn drop(&mut self) {
        self.cancel_reload.cancel();
    }
}

impl<T> Acceptor for OpensslAcceptor<T>
where
    T: Acceptor + Send + 'static,
    T::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Coupler = TcpCoupler<Self::Stream>;
    type Stream = HandshakeStream<SslStream<T::Stream>>;

    /// Get the local address bound to this listener.
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
            return Err(IoError::other("openssl: no active tls config"));
        };
        let conn = async move {
            let ssl = Ssl::new(tls_acceptor.context()).map_err(IoError::other)?;
            let mut tls_stream = SslStream::new(ssl, stream).map_err(IoError::other)?;
            std::pin::Pin::new(&mut tls_stream)
                .accept()
                .await
                .map_err(IoError::other)?;
            Ok(tls_stream)
        };

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

