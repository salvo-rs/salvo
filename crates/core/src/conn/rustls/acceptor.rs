//! rustls module
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::task::noop_waker_ref;
use futures_util::{Stream, StreamExt};
use pin_project::pin_project;
use tokio::net::ToSocketAddrs;
use tokio_rustls::server::TlsStream;

use crate::async_trait;
use crate::conn::{Accepted, Acceptor, IntoConfigStream, SocketAddr, TcpListener, TlsConnStream};

use super::RustlsConfig;

/// RustlsAcceptor

#[pin_project]
pub struct RustlsAcceptor<C, T> {
    #[pin]
    config_stream: C,
    inner: T,
    tls_acceptor: Option<tokio_rustls::TlsAcceptor>,
}

impl<C, T> RustlsListener<C, T>
where
    C: Stream<Item = RustlsConfig> + Send + 'static,
    T: Acceptor,
{
    /// Create a new `RustlsListener`.
    #[inline]
    pub fn new(config: C, inner: T) -> RustlsListener<C, T> {
        Self::try_new(config, inner).unwrap()
    }
    /// Try to create a new `RustlsListener`.
    #[inline]
    pub fn try_new(config: C, inner: T) -> IoResult<RustlsListener<C, T>> {
        Ok(RustlsListener {
            config_stream: config.into(),
            inner,
            tls_acceptor: None,
        })
    }
}

impl<C> RustlsListener<C, TcpListener>
where
    C: IntoConfigStream<RustlsConfig>,
{
    /// Bind to socket address.
    #[inline]
    pub async fn bind(config: C, addr: impl ToSocketAddrs) -> RustlsListener<C::Stream, TcpListener> {
        Self::try_bind(config, addr).await.unwrap()
    }
    /// Try bind to socket address.
    #[inline]
    pub async fn try_bind(config: C, addr: impl ToSocketAddrs) -> IoResult<RustlsListener<C::Stream, TcpListener>> {
        let config_stream = config.into_stream();
        let inner = TcpListener::try_bind(addr).await?;
        Ok(RustlsListener {
            config_stream,
            inner,
            tls_acceptor: None,
        })
    }
}

#[async_trait]
impl<C, T> Acceptor for RustlsListener<C, T>
where
    C: Stream<Item = RustlsConfig> + Send + Unpin + 'static,
    T: Acceptor,
{
    type Conn = TlsConnStream<TlsStream<T::Conn>>;

    fn local_addrs(&self) -> Vec<&SocketAddr> {
        self.inner.local_addrs()
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
            let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config.build_server_config()?));
            if self.tls_acceptor.is_some() {
                tracing::info!("tls config changed.");
            } else {
                tracing::info!("tls config loaded.");
            }
            self.tls_acceptor = Some(tls_acceptor);
        }
        let Accepted {
            stream,
            local_addr,
            remote_addr,
        } = self.inner.accept().await?;
        let tls_acceptor = match &self.tls_acceptor {
            Some(tls_acceptor) => tls_acceptor,
            None => return Err(IoError::new(ErrorKind::Other, "no valid tls config.")),
        };

        let stream = TlsConnStream::new(tls_acceptor.accept(stream));
        Ok(Accepted {
            stream,
            local_addr,
            remote_addr,
        })
    }
}
