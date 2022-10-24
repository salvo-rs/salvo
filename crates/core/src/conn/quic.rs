//! QuicListener and it's implements.
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::net::ToSocketAddrs;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::vec;

use bytes::Bytes;
use futures_util::StreamExt;
pub use h3_quinn::quinn::ServerConfig;
use h3_quinn::quinn::{Endpoint, EndpointConfig, Incoming};
use h3_quinn::NewConnection;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::async_trait;
use crate::conn::{HttpBuilders, SocketAddr};
use crate::http::version::{self, HttpConnection, Version};
use crate::service::HyperHandler;

use super::{Accepted, Acceptor, Listener};

/// QuicListener
pub struct QuicListener<T> {
    addr: T,
    config: ServerConfig,
}
impl<T: ToSocketAddrs> QuicListener<T> {
    /// Bind to socket address.
    #[inline]
    pub fn bind(addr: T, config: ServerConfig) -> Self {
        QuicListener { addr, config }
    }
}
#[async_trait]
impl<T> Listener for QuicListener<T>
where
    T: ToSocketAddrs + Send,
{
    type Acceptor = QuicAcceptor;
    async fn into_acceptor(self) -> IoResult<Self::Acceptor> {
        let socket = std::net::UdpSocket::bind(self.addr)?;
        let local_addr: SocketAddr = socket.local_addr()?.into();
        let (endpoint, incoming) = Endpoint::new(EndpointConfig::default(), Some(self.config), socket)?;
        Ok(QuicAcceptor {
            endpoint,
            incoming,
            local_addr,
        })
    }
}

pub struct QuicAcceptor {
    endpoint: Endpoint,
    incoming: Incoming,
    local_addr: SocketAddr,
}

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
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<IoResult<()>> {
        unimplemented!()
    }
}

impl AsyncWrite for H3Connection {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        unimplemented!()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        unimplemented!()
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        unimplemented!()
    }
}

#[async_trait]
impl HttpConnection for H3Connection {
    async fn http_version(&mut self) -> Option<Version> {
        Some(Version::HTTP_3)
    }
    async fn serve(self, handler: HyperHandler, builders: Arc<HttpBuilders>) -> IoResult<()> {
        builders.http3.serve_connection(self, handler).await
    }
}

#[async_trait]
impl Acceptor for QuicAcceptor {
    type Conn = H3Connection;

    #[inline]
    fn local_addrs(&self) -> Vec<&SocketAddr> {
        vec![&self.local_addr]
    }

    #[inline]
    async fn accept(&mut self) -> IoResult<Accepted<Self::Conn>> {
        println!("accept......0");
        while let Some(new_conn) = self.incoming.next().await {
            println!("accept.....1");
            let remote_addr = new_conn.remote_address();
            match new_conn.await {
                Ok(conn) => {
                    println!("=========================4");
                    let conn = h3::server::Connection::new(h3_quinn::Connection::new(conn))
                        .await
                        .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))?;
                        println!("=========================5");
                    return Ok(Accepted {
                        conn: H3Connection(conn),
                        local_addr: self.local_addr.clone(),
                        remote_addr: remote_addr.into(),
                    });
                }
                Err(e) => return Err(IoError::new(ErrorKind::Other, e.to_string())),
            }
        }
        Err(IoError::new(ErrorKind::Other, "http3 accept error"))
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{Stream, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::QuicStream;

    use super::*;

    #[tokio::test]
    async fn test_Quic_listener() {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 6878));

        let mut listener = QuicListener::bind(addr);
        tokio::spawn(async move {
            let mut stream = QuicStream::connect(addr).await.unwrap();
            stream.write_i32(150).await.unwrap();
        });

        let mut stream = listener.next().await.unwrap().unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 150);
    }
}
