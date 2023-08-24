//! rustls module
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::stream::BoxStream;
use futures_util::task::noop_waker_ref;
use futures_util::{Stream, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::server::TlsStream;

use crate::async_trait;
use crate::conn::Holding;
use crate::conn::{Accepted, Acceptor, IntoConfigStream, Listener};
use crate::http::uri::Scheme;
use crate::http::Version;

use super::ServerConfig;

/// RustlsListener
pub struct RustlsListener<S, C, T> {
    config_stream: S,
    inner: T,
    _config: PhantomData<C>,
}

impl<S, C, T> RustlsListener<S, C, T>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<ServerConfig, Error = IoError> + Send + 'static,
    T: Listener + Send,
{
    /// Create a new `RustlsListener`.
    #[inline]
    pub fn new(config_stream: S, inner: T) -> Self {
        RustlsListener {
            config_stream,
            inner,
            _config: PhantomData,
        }
    }
}

#[async_trait]
impl<S, C, T> Listener for RustlsListener<S, C, T>
where
    S: IntoConfigStream<C> + Send + 'static,
    C: TryInto<ServerConfig, Error = IoError> + Send + 'static,
    T: Listener + Send,
    T::Acceptor: Send + 'static,
{
    type Acceptor = RustlsAcceptor<BoxStream<'static, C>, C, T::Acceptor>;

    async fn bind(self) -> Self::Acceptor {
        self.try_bind().await.unwrap()
    }

    async fn try_bind(self) -> IoResult<Self::Acceptor> {
        Ok(RustlsAcceptor::new(
            self.config_stream.into_stream().boxed(),
            self.inner.try_bind().await?,
        ))
    }
}

/// RustlsAcceptor
pub struct RustlsAcceptor<S, C, T> {
    config_stream: S,
    inner: T,
    holdings: Vec<Holding>,
    tls_acceptor: Option<tokio_rustls::TlsAcceptor>,
    _config: PhantomData<C>,
}
impl<S, C, T> RustlsAcceptor<S, C, T>
where
    S: futures_util::Stream<Item = C> + Send + 'static,
    C: TryInto<ServerConfig, Error = IoError> + Send + 'static,
    T: Acceptor + Send,
{
    /// Create a new `RustlsAcceptor`.
    pub fn new(config_stream: S, inner: T) -> RustlsAcceptor<S, C, T> {
        let holdings = inner
            .holdings()
            .iter()
            .map(|h| {
                let mut versions = h.http_versions.clone();
                #[cfg(feature = "http1")]
                if !versions.contains(&Version::HTTP_11) {
                    versions.push(Version::HTTP_11);
                }
                #[cfg(feature = "http2")]
                if !versions.contains(&Version::HTTP_2) {
                    versions.push(Version::HTTP_2);
                }
                Holding {
                    local_addr: h.local_addr.clone(),
                    http_versions: versions,
                    http_scheme: Scheme::HTTPS,
                }
            })
            .collect();
        RustlsAcceptor {
            config_stream,
            inner,
            holdings,
            tls_acceptor: None,
            _config: PhantomData,
        }
    }
}

#[async_trait]
impl<S, C, T> Acceptor for RustlsAcceptor<S, C, T>
where
    S: Stream<Item = C> + Send + Unpin + 'static,
    C: TryInto<ServerConfig, Error = IoError> + Send + 'static,
    T: Acceptor + Send + 'static,
    <T as Acceptor>::Conn: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Conn = TlsStream<T::Conn>;

    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(&mut self) -> IoResult<Accepted<Self::Conn>> {
        let config = {
            let mut config = None;
            while let Poll::Ready(Some(item)) =
                Pin::new(&mut self.config_stream).poll_next(&mut Context::from_waker(noop_waker_ref()))
            {
                config = Some(item);
            }
            config
        };
        if let Some(config) = config {
            let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config.try_into()?));
            if self.tls_acceptor.is_some() {
                tracing::info!("tls config changed.");
            } else {
                tracing::info!("tls config loaded.");
            }
            self.tls_acceptor = Some(tls_acceptor);
        }
        let tls_acceptor = match &self.tls_acceptor {
            Some(tls_acceptor) => tls_acceptor,
            None => return Err(IoError::new(ErrorKind::Other, "rustls: invalid tls config.")),
        };

        let Accepted {
            conn,
            local_addr,
            remote_addr,
            http_version,
            http_scheme,
        } = self.inner.accept().await?;
        let conn = tls_acceptor
            .accept(conn)
            .await
            .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))?;
        Ok(Accepted {
            conn,
            local_addr,
            remote_addr,
            http_version,
            http_scheme,
        })
    }
}
