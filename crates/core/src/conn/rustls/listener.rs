//! rustls module
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::sync::Arc;

use futures_util::{Stream, StreamExt};
use tokio::net::ToSocketAddrs;
pub use tokio_rustls::rustls::server::ServerConfig;
use tokio_rustls::server::TlsStream;

use crate::async_trait;
use crate::conn::{Accepted, Acceptor, HandshakeStream, IntoConfigStream, SocketAddr, TcpListener};

use super::RustlsConfig;

/// RustlsListener
pub struct RustlsListener<C, T> {
    config_stream: C,
    inner: T,
    server_config: Option<ServerConfig>,
    tls_acceptor: Option<tokio_rustls::TlsAcceptor>,
}

impl<C, T> RustlsListener<C, T>
where
    C: Stream<Item = RustlsConfig> + Send + Unpin + 'static,
    T: Acceptor,
{
    #[inline]
    pub fn new(config: impl IntoConfigStream<RustlsConfig>, inner: T) -> RustlsListener<C, T> {
        Self::try_new(config, inner).unwrap()
    }
    #[inline]
    pub fn try_new(config: impl IntoConfigStream<RustlsConfig>, inner: T) -> IoResult<RustlsListener<C, T>> {
        let config_stream = config.into_stream()?;
        Ok(RustlsListener {
            config_stream,
            inner,
            server_config: None,
            tls_acceptor: None,
        })
    }
}

impl<C> RustlsListener<C, TcpListener>
where
    C: Stream<Item = RustlsConfig> + Send + Unpin + 'static,
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
            server_config: None,
            inner,
            tls_acceptor: None,
        })
    }
}

#[async_trait]
impl<C, T> Acceptor for RustlsListener<C, T>
where
    C: IntoConfigStream<ServerConfig>,
    T: Acceptor,
{
    type Conn = HandshakeStream<TlsStream<T::Conn>>;
    type Error = IoError;

    fn local_addrs(&self) -> Vec<&SocketAddr> {
        self.inner.local_addrs()
    }

    #[inline]
    async fn accept(&mut self) -> Result<Accepted<Self::Conn>, Self::Error> {
        loop {
            tokio::select! {
                tls_config = self.config_stream.next() => {
                    if let Some(tls_config) = tls_config {
                        match tls_config.create_server_config() {
                            Ok(server_config) => {
                                if self.tls_acceptor.is_some() {
                                    tracing::info!("tls config changed.");
                                } else {
                                    tracing::info!("tls config loaded.");
                                }
                                self.tls_acceptor = Some(tokio_rustls::TlsAcceptor::from(Arc::new(server_config)));

                            },
                            Err(err) => tracing::error!(error = %err, "invalid tls config."),
                        }
                    } else {
                        unreachable!()
                    }
                }
                accepted = self.inner.accept() => {
                    let (stream, local_addr, remote_addr, _) = accepted?;
                    let tls_acceptor = match &self.tls_acceptor {
                        Some(tls_acceptor) => tls_acceptor,
                        None => return Err(IoError::new(ErrorKind::Other, "no valid tls config.")),
                    };

                    let stream = HandshakeStream::new(tls_acceptor.accept(stream));
                    return Ok(Accepted{stream, local_addr, remote_addr});
                }
            }
        }
    }
}
