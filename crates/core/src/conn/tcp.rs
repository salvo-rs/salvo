//! Listener trait and it's implements.
use std::io::Error as IoError;
use std::vec;

use tokio::io::Result as IoResult;
use tokio::net::{TcpListener as TokioTcpListener, TcpStream, ToSocketAddrs};

use crate::async_trait;
use crate::conn::SocketAddr;

use super::{Accepted, Acceptor, Listener};

/// TcpListener
pub struct TcpListener {
    inner: TokioTcpListener,
    local_addr: SocketAddr,
}
impl TcpListener {
    /// Bind to socket address.
    #[inline]
    pub async fn bind(addr: impl ToSocketAddrs) -> Self {
        Self::try_bind(addr).await.unwrap()
    }

    /// Try to bind to socket address.
    #[inline]
    pub async fn try_bind(addr: impl ToSocketAddrs) -> IoResult<Self> {
        let inner = TokioTcpListener::bind(addr).await?;
        let local_addr: SocketAddr = inner.local_addr()?.into();
        Ok(TcpListener { inner, local_addr })
    }
}
impl Listener for TcpListener {}

#[async_trait]
impl Acceptor for TcpListener {
    type Conn = TcpStream;
    type Error = IoError;

    #[inline]
    fn local_addrs(&self) -> Vec<&SocketAddr> {
        vec![&self.local_addr]
    }

    #[inline]
    async fn accept(&self) -> Result<Accepted<Self::Conn>, Self::Error> {
        let local_addr = self.local_addr.clone();
        self.inner.accept().await.map(move |(stream, remote_addr)| Accepted {
            stream,
            local_addr,
            remote_addr: remote_addr.into(),
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

        let mut listener = TcpListener::bind(addr);
        tokio::spawn(async move {
            let mut stream = TcpStream::connect(addr).await.unwrap();
            stream.write_i32(150).await.unwrap();
        });

        let mut stream = listener.next().await.unwrap().unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 150);
    }
}
