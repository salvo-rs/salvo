//! rustls module
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::sync::Arc;

use futures_util::{Stream, StreamExt};
use tokio::net::ToSocketAddrs;
pub use tokio_rustls::rustls::server::ServerConfig;
use tokio_rustls::server::TlsStream;

use crate::async_trait;
use crate::conn::{Accepted, Acceptor, TlsConnStream, SocketAddr, TcpListener};

/// RustlsListener
pub struct RustlsListener<C, T> {
    config_stream: C,
    inner: T,
    tls_acceptor: Option<tokio_rustls::TlsAcceptor>,
}

impl<C, T> RustlsListener<C, T>
where
    C: Stream + Send + 'static,
    C::Item: Into<ServerConfig>,
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
    C: Stream + Send + 'static,
    C::Item: Into<ServerConfig>,
{
    /// Bind to socket address.
    #[inline]
    pub async fn bind(config_stream: C, addr: impl ToSocketAddrs) -> RustlsListener<C, TcpListener> {
        Self::try_bind(config_stream, addr).await.unwrap()
    }
    /// Try to bind to socket address.
    #[inline]
    pub async fn try_bind(config_stream: C, addr: impl ToSocketAddrs) -> IoResult<RustlsListener<C, TcpListener>> {
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
    C: Stream + Send  + Unpin + 'static,
    C::Item: Into<ServerConfig>,
    T: Acceptor,
{
    type Conn = TlsConnStream<TlsStream<T::Conn>>;
    type Error = IoError;

    fn local_addrs(&self) -> Vec<&SocketAddr> {
        self.inner.local_addrs()
    }

    #[inline]
    async fn accept(&mut self) -> Result<Accepted<Self::Conn>, Self::Error> {
        loop {
            tokio::select! {
                server_config = self.config_stream.next() => {
                    if let Some(server_config) = server_config {
                        if self.tls_acceptor.is_some() {
                            tracing::info!("tls config changed.");
                        } else {
                            tracing::info!("tls config loaded.");
                        }
                        self.tls_acceptor = Some(tokio_rustls::TlsAcceptor::from(Arc::new(server_config.into())));
                    } else {
                        unreachable!()
                    }
                }
                accepted = self.inner.accept() => {
                    let Accepted{stream, local_addr, remote_addr} = accepted.map_err(|e|IoError::new(ErrorKind::Other, format!("accept error: {}", e)))?;
                    let tls_acceptor = match &self.tls_acceptor {
                        Some(tls_acceptor) => tls_acceptor,
                        None => return Err(IoError::new(ErrorKind::Other, "no valid tls config.")),
                    };

                    let stream = TlsConnStream::new(tls_acceptor.accept(stream));
                    return Ok(Accepted{stream, local_addr, remote_addr});
                }
            }
        }
    }
}
