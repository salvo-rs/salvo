//! QuinnListener and it's implements.
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::net::ToSocketAddrs;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use std::vec;

use http::uri::Scheme;
pub use salvo_http3::http3_quinn::ServerConfig;
use salvo_http3::http3_quinn::{self, Endpoint};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::sync::CancellationToken;

use crate::async_trait;
use crate::conn::rustls::RustlsConfig;
use crate::conn::Holding;
use crate::conn::HttpBuilder;
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
        let socket = local_addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| IoError::new(ErrorKind::AddrNotAvailable, "No address available"))?;
        let holding = Holding {
            local_addr: socket.into(),
            http_versions: vec![Version::HTTP_3],
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
pub struct H3Connection(http3_quinn::Connection);
impl H3Connection {
    /// Get inner quinn connection.
    pub fn into_inner(self) -> http3_quinn::Connection {
        self.0
    }
}
impl Deref for H3Connection {
    type Target = http3_quinn::Connection;
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
    async fn serve(
        self,
        handler: HyperHandler,
        builder: Arc<HttpBuilder>,
        server_shutdown_token: CancellationToken,
        idle_connection_timeout: Option<Duration>,
    ) -> IoResult<()> {
        builder.quinn.serve_connection(self, handler, server_shutdown_token, idle_connection_timeout).await
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
                    let conn = http3_quinn::Connection::new(conn);
                    return Ok(Accepted {
                        conn: H3Connection(conn),
                        local_addr: self.holdings[0].local_addr.clone(),
                        remote_addr: remote_addr.into(),
                        http_scheme: self.holdings[0].http_scheme.clone(),
                        http_version: Version::HTTP_3,
                    });
                }
                Err(e) => return Err(IoError::new(ErrorKind::Other, e.to_string())),
            }
        }
        Err(IoError::new(ErrorKind::Other, "quinn accept error"))
    }
}
