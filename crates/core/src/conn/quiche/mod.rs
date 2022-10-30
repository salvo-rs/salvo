//! QuicheListener and it's implements.
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::net::ToSocketAddrs;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::vec;

use bytes::Bytes;
use futures_util::StreamExt;
pub use salvo_quinn::quinn::ServerConfig;
use salvo_quinn::quinn::{Endpoint, EndpointConfig, Incoming};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use http::uri::Scheme;

use crate::async_trait;
use crate::conn::rustls::RustlsConfig;
use crate::conn::Holding;
use crate::conn::HttpBuilders;
use crate::http::{HttpConnection, Version};
use crate::service::HyperHandler;
pub use quiche::Config;

use super::{Accepted, Acceptor, Listener};

mod builder;
pub use builder::Builder;

/// QuicheListener
pub struct QuicheListener<C,T> {
    config_stream: C,
    local_addr: T,
}
impl<T: ToSocketAddrs> QuicheListener<T> {
    /// Bind to socket address.
    #[inline]
    pub fn new(config_stream: C, local_addr: T) -> Self {
        QuicheListener { config_stream, local_addr }
    }
}
#[async_trait]
impl<T> Listener for QuicheListener<T>
where
    T: ToSocketAddrs + Send,
{
    type Acceptor = QuicheAcceptor<BoxStream<'static, RustlsConfig>;
    
    async fn bind(self) -> Self::Acceptor {
        self.try_bind().await.unwrap()
    }

    async fn try_bind(self) -> IoResult<Self::Acceptor> {
        let Self { local_addr, config } = self;
        let socket = mio::net::UdpSocket::bind(local_addr)?;
        let holding = Holding {
            local_addr: socket.local_addr()?.into(),
            http_version: Version::HTTP_3,
            http_scheme: Scheme::HTTPS,
        };
        Ok(QuicheAcceptor {
            config_stream:
            self.config_stream.into_stream().boxed(),
            socket,
            holdings: vec![holding],
        })
    }
}

/// QuicheAcceptor
pub struct QuicheAcceptor<C> {
    config_stream: C,
    holdings: Vec<Holding>,
    inner_acceptor: Option<InnerAcceptor>,
}

/// Http3 Connection.
pub struct H3Connection(pub salvo_quinn::server::Connection<salvo_quinn::quinn_impl::Connection, Bytes>);
impl Deref for H3Connection {
    type Target = salvo_quinn::server::Connection<salvo_quinn::quinn_impl::Connection, Bytes>;
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
impl Acceptor for QuicheAcceptor {
    type Conn = H3Connection;

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
            let endpoint = Endpoint::from(Arc::new(config.build_server_config()?));
            if self.endpoint.is_some() {
                tracing::info!("tls config changed.");
            } else {
                tracing::info!("tls config loaded.");
            }
            self.endpoint = Some(endpoint);
        }
        if let Some(conn) = self.endpoint.next().await {
            let remote_addr = conn.remote_address();
            match conn.await {
                Ok(conn) => {
                    return Ok(Accepted {
                        conn,
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
