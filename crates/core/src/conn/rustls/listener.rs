//! rustls module
use std::error::Error as StdError;
use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, Result as IoResult};
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::stream::{BoxStream, Stream, StreamExt};
use futures_util::task::noop_waker_ref;
use tokio_rustls::server::TlsStream;

use crate::conn::tcp::{DynTcpAcceptor, TcpCoupler, ToDynTcpAcceptor};
use crate::conn::{Accepted, Acceptor, HandshakeStream, Holding, IntoConfigStream, Listener};
use crate::fuse::ArcFuseFactory;
use crate::http::uri::Scheme;

use super::ServerConfig;

/// A wrapper of `Listener` with rustls.
pub struct RustlsListener<S, C, T, E> {
    config_stream: S,
    inner: T,
    _phantom: PhantomData<(C, E)>,
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
            _phantom: PhantomData,
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
    type Acceptor = RustlsAcceptor<BoxStream<'static, C>, C, T::Acceptor, E>;

    async fn try_bind(self) -> crate::Result<Self::Acceptor> {
        Ok(RustlsAcceptor::new(
            self.config_stream.into_stream().boxed(),
            self.inner.try_bind().await?,
        ))
    }
}

/// A wrapper of `Acceptor` with rustls.
pub struct RustlsAcceptor<S, C, T, E> {
    config_stream: S,
    inner: T,
    holdings: Vec<Holding>,
    tls_acceptor: Option<tokio_rustls::TlsAcceptor>,
    _phantom: PhantomData<(C, E)>,
}

impl<S, C, T, E> Debug for RustlsAcceptor<S, C, T, E> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RustlsAcceptor").finish()
    }
}

impl<S, C, T, E> RustlsAcceptor<S, C, T, E>
where
    S: Stream<Item = C> + Unpin + Send + 'static,
    C: TryInto<ServerConfig, Error = E> + Send + 'static,
    T: Acceptor + Send + 'static,
    T::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    E: StdError + Send + 'static,
{
    /// Create a new `RustlsAcceptor`.
    pub fn new(config_stream: S, inner: T) -> Self {
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
            config_stream,
            inner,
            holdings,
            tls_acceptor: None,
            _phantom: PhantomData,
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

impl<S, C, T, E> Acceptor for RustlsAcceptor<S, C, T, E>
where
    S: Stream<Item = C> + Send + Unpin + 'static,
    C: TryInto<ServerConfig, Error = E> + Send + 'static,
    T: Acceptor + Send + 'static,
    <T as Acceptor>::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    E: StdError + Send,
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
        let config = {
            let mut config = None;
            while let Poll::Ready(Some(item)) = Pin::new(&mut self.config_stream)
                .poll_next(&mut Context::from_waker(noop_waker_ref()))
            {
                config = Some(item);
            }
            config
        };
        if let Some(config) = config {
            let config: ServerConfig = config
                .try_into()
                .map_err(|e| IoError::other(e.to_string()))?;
            let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
            if self.tls_acceptor.is_some() {
                tracing::info!("tls config changed.");
            } else {
                tracing::info!("tls config loaded.");
            }
            self.tls_acceptor = Some(tls_acceptor);
        }
        let Some(tls_acceptor) = &self.tls_acceptor else {
            return Err(IoError::other("rustls: invalid tls config."));
        };

        let Accepted {
            coupler: _,
            stream,
            fusewire,
            local_addr,
            remote_addr,
            ..
        } = self.inner.accept(fuse_factory).await?;
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
