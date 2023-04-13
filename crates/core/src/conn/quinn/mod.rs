//! QuinnListener and it's implements.
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::net::ToSocketAddrs;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::vec;

use bytes::Bytes;
use h3_quinn::quinn::Endpoint;
pub use h3_quinn::quinn::ServerConfig;
use http::uri::Scheme;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::async_trait;
use crate::conn::rustls::RustlsConfig;
use crate::conn::Holding;
use crate::conn::HttpBuilders;
use crate::http::{HttpConnection, Version};
use crate::service::HyperHandler;

use super::{Accepted, Acceptor, Listener};

mod builder;
pub use builder::Builder;

/// QuinnListener
pub struct QuinnListener<T> {
    config: RustlsConfig,
    local_addr: T,
}
impl<T: ToSocketAddrs> QuinnListener<T> {
    /// Bind to socket address.
    #[inline]
    pub fn new(config: RustlsConfig, local_addr: T) -> Self {
        let config = config.alpn_protocols([b"h3-29".to_vec(), b"h3-28".to_vec(), b"h3-27".to_vec(), b"h3".to_vec()]);
        QuinnListener { config, local_addr }
    }
}
#[async_trait]
impl<T> Listener for QuinnListener<T>
where
    T: ToSocketAddrs + Send,
{
    type Acceptor = QuinnAcceptor;

    async fn bind(self) -> Self::Acceptor {
        self.try_bind().await.unwrap()
    }

    async fn try_bind(self) -> IoResult<Self::Acceptor> {
        let Self { local_addr, config } = self;
        let socket = local_addr.to_socket_addrs()?.next().ok_or_else(|| IoError::new(ErrorKind::AddrNotAvailable, "No address available"))?;
        let holding = Holding {
            local_addr: socket.into(),
            http_version: Version::HTTP_3,
            http_scheme: Scheme::HTTPS,
        };
        let crypto = config.build_server_config()?;
        let server_config = crate::conn::quinn::ServerConfig::with_crypto(Arc::new(crypto));
        let endpoint = Endpoint::server(server_config, socket)?;
        Ok(QuinnAcceptor {
            endpoint,
            holdings: vec![holding],
        })
    }
}

/// QuinnAcceptor
pub struct QuinnAcceptor {
    endpoint: Endpoint,
    holdings: Vec<Holding>,
}

/// Http3 Connection.
pub struct H3Connection(pub h3::server::Connection<h3_quinn::Connection, Bytes>);
impl Deref for H3Connection {
    type Target = h3::server::Connection<h3_quinn::Connection, Bytes>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for H3Connection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl AsyncRead for H3Connection {
    fn poll_read(self: Pin<&mut Self>, _cx: &mut Context<'_>, _buf: &mut ReadBuf<'_>) -> Poll<IoResult<()>> {
        unimplemented!()
    }
}

impl AsyncWrite for H3Connection {
    fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, _buf: &[u8]) -> Poll<IoResult<usize>> {
        unimplemented!()
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        unimplemented!()
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        unimplemented!()
    }
}

#[async_trait]
impl HttpConnection for H3Connection {
    async fn version(&mut self) -> Option<Version> {
        Some(Version::HTTP_3)
    }
    async fn serve(self, handler: HyperHandler, builders: Arc<HttpBuilders>) -> IoResult<()> {
        builders.quinn.serve_connection(self, handler).await
    }
}

#[async_trait]
impl Acceptor for QuinnAcceptor {
    type Conn = H3Connection;

    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(&mut self) -> IoResult<Accepted<Self::Conn>> {
        if let Some(new_conn) = self.endpoint.accept().await {
            let remote_addr = new_conn.remote_address();
            match new_conn.await {
                Ok(conn) => {
                    let conn = h3::server::Connection::new(h3_quinn::Connection::new(conn))
                        .await
                        .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))?;
                    return Ok(Accepted {
                        conn: H3Connection(conn),
                        local_addr: self.holdings[0].local_addr.clone(),
                        remote_addr: remote_addr.into(),
                        http_scheme: self.holdings[0].http_scheme.clone(),
                        http_version: self.holdings[0].http_version,
                    });
                }
                Err(e) => return Err(IoError::new(ErrorKind::Other, e.to_string())),
            }
        }
        Err(IoError::new(ErrorKind::Other, "quinn accept error"))
    }
}
