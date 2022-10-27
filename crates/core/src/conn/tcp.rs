//! TcpListener and it's implements.
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::sync::Arc;
use std::vec;

use tokio::net::{TcpListener as TokioTcpListener, TcpStream, ToSocketAddrs};

use crate::async_trait;
use crate::conn::HttpBuilders;
use crate::conn::{Holding};
use crate::http::{HttpConnection, Version};
use crate::http::uri::Scheme;
use crate::service::HyperHandler;

use super::{Accepted, Acceptor, IntoAcceptor, Listener};

/// TcpListener
pub struct TcpListener<T> {
    local_addr: T,
}
impl<T: ToSocketAddrs> TcpListener<T> {
    /// Bind to socket address.
    #[inline]
    pub fn bind(local_addr: T) -> Self {
        TcpListener { local_addr }
    }
}
#[async_trait]
impl<T> IntoAcceptor for TcpListener<T>
where
    T: ToSocketAddrs + Send,
{
    type Acceptor = TcpAcceptor;
    async fn into_acceptor(self) -> IoResult<Self::Acceptor> {
        let inner = TokioTcpListener::bind(self.local_addr).await?;
        let holding = Holding {
            local_addr: inner.local_addr()?.into(),
            http_version: Version::HTTP_11,
            http_scheme: Scheme::HTTP,
        };

        Ok(TcpAcceptor {
            inner,
            holdings: vec![holding],
        })
    }
}
impl<T> Listener for TcpListener<T> where T: ToSocketAddrs + Send {}

pub struct TcpAcceptor {
    inner: TokioTcpListener,
    holdings: Vec<Holding>,
}

#[async_trait]
impl HttpConnection for TcpStream {
    async fn http_version(&mut self) -> Option<Version> {
        Some(Version::HTTP_11)
    }
    async fn serve(self, handler: HyperHandler, builders: Arc<HttpBuilders>) -> IoResult<()> {
        builders
            .http1
            .serve_connection(self, handler)
            .with_upgrades()
            .await
            .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))
    }
}

#[async_trait]
impl Acceptor for TcpAcceptor {
    type Conn = TcpStream;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(&mut self) -> IoResult<Accepted<Self::Conn>> {
        self.inner.accept().await.map(move |(conn, remote_addr)| Accepted {
            conn,
            local_addr: self.holdings[0].local_addr.clone(),
            remote_addr: remote_addr.into(),
            http_version: self.holdings[0].http_version.clone(),
            http_scheme: self.holdings[0].http_scheme.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{Stream, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;

    #[tokio::test]
    async fn test_tcp_listener() {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 6878));

        let listener = TcpListener::bind(addr);
        let mut acceptor = listener.into_acceptor().await.unwrap();
        let addr = acceptor.local_addrs().remove(0).into_std().unwrap();
        tokio::spawn(async move {
            let mut stream = TcpStream::connect(addr).await.unwrap();
            stream.write_i32(150).await.unwrap();
        });

        let Accepted { mut conn, .. } = acceptor.accept().await.unwrap();
        assert_eq!(conn.read_i32().await.unwrap(), 150);
    }
}
