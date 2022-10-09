//! rustls module
use std::collections::HashMap;
use std::fmt::{self, Formatter};
use std::fs::File;
use std::future::Future;
use std::io::{self, Error as IoError, Read};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::future::Ready;
use futures_util::{ready, stream, Stream};
use pin_project::pin_project;
use tokio::net::{ToSocketAddrs, TcpListener as TokioTcpListener};
use tokio::io::{AsyncRead, AsyncWrite, ErrorKind, ReadBuf};
pub use tokio_rustls::rustls::server::ServerConfig;
use tokio_rustls::rustls::server::{
    AllowAnyAnonymousOrAuthenticatedClient, AllowAnyAuthenticatedClient, ClientHello, NoClientAuth, ResolvesServerCert,
};
use tokio_rustls::rustls::sign::{self, CertifiedKey};
use tokio_rustls::rustls::{Certificate, PrivateKey, RootCertStore};
use tokio_rustls::server::TlsStream;

use crate::conn::{SocketAddr, Acceptor, Listener, Accepted, HandshakeStream, TcpListener, IntoConfigStream};
use crate::async_trait;

use super::{RustlsConfig};

/// RustlsListener
pub struct RustlsListener<C, T> {
    config_stream: C,
    server_config: Option<Arc<ServerConfig>>,
    inner: T,
    current_tls_acceptor: Option<tokio_rustls::TlsAcceptor>,
}

impl<C> RustlsListener<C, TcpListener>
where
    C: IntoConfigStream<Arc<ServerConfig>>,
{
    /// Bind to socket address.
    #[inline]
    pub async fn bind(config_stream: C, addr: impl ToSocketAddrs) -> RustlsListener<C, TcpListener> {
        Self::try_bind(config_stream, addr).unwrap()
    }
    /// Try to bind to socket address.
    #[inline]
    pub async fn try_bind(config_stream: C, addr: impl ToSocketAddrs) -> Result<RustlsListener<C, TcpListener>, hyper::Error> {
        let inner = TcpListener::bind(addr).await?;
        Ok(RustlsListener {
            config_stream,
            server_config: None,
            inner,
        })
    }
}

#[async_trait]
impl<C, T> Acceptor for RustlsListener< C, T>
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
    async fn accept(&self) -> Result<Accepted<Self::Conn>, Self::Error> {
        loop {
            tokio::select! {
                tls_config = self.config_stream.next() => {
                    if let Some(tls_config) = tls_config {
                        match tls_config.create_server_config() {
                            Ok(server_config) => {
                                if self.current_tls_acceptor.is_some() {
                                    tracing::info!("tls config changed.");
                                } else {
                                    tracing::info!("tls config loaded.");
                                }
                                self.current_tls_acceptor = Some(tokio_rustls::TlsAcceptor::from(Arc::new(server_config)));
    
                            },
                            Err(err) => tracing::error!(error = %err, "invalid tls config."),
                        }
                    } else {
                        unreachable!()
                    }
                }
                accepted = self.inner.accept() => {
                    let (stream, local_addr, remote_addr, _) = accepted?;
                    let tls_acceptor = match &self.current_tls_acceptor {
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
