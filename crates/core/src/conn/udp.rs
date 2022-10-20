//! Listener trait and it's implements.
use std::io::Result as IoResult;
use std::vec;

use tokio::net::{UdpListener as TokioUdpListener, UdpStream, ToSocketAddrs};

use crate::async_trait;
use crate::conn::SocketAddr;

use super::{Accepted, Acceptor, Listener, CommProtocol};

/// UdpListener
pub struct UdpListener<T> {
    addr: T,
}
impl<T: ToSocketAddrs> UdpListener<T> {
    /// Bind to socket address.
    #[inline]
    pub fn bind(addr: T) -> Self {
        UdpListener { addr }
    }
}
#[async_trait]
impl<T> Listener for UdpListener<T>
where
    T: ToSocketAddrs + Send,
{
    type Acceptor = UdpAcceptor;
    fn proto() -> CommProtocol {
        CommProtocol::Udp
    }
    async fn into_acceptor(self) -> IoResult<Self::Acceptor> {
        let inner = TokioUdpListener::bind(self.addr).await?;
        let local_addr: SocketAddr = inner.local_addr()?.into();
        Ok(UdpAcceptor { inner, local_addr })
    }
}

pub struct UdpAcceptor {
    inner: TokioUdpListener,
    local_addr: SocketAddr,
}

#[async_trait]
impl Acceptor for UdpAcceptor {
    type Conn = UdpStream;

    #[inline]
    fn local_addrs(&self) -> Vec<&SocketAddr> {
        vec![&self.local_addr]
    }

    #[inline]
    async fn accept(&mut self) -> IoResult<Accepted<Self::Conn>> {
        self.inner.accept().await.map(move |(stream, remote_addr)| Accepted {
            stream,
            local_addr: self.local_addr.clone(),
            remote_addr: remote_addr.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use futures_util::{Stream, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UdpStream;

    use super::*;

    #[tokio::test]
    async fn test_Udp_listener() {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 6878));

        let mut listener = UdpListener::bind(addr);
        tokio::spawn(async move {
            let mut stream = UdpStream::connect(addr).await.unwrap();
            stream.write_i32(150).await.unwrap();
        });

        let mut stream = listener.next().await.unwrap().unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 150);
    }
}
