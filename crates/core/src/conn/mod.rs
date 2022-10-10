//! Listener trait and it's implements.
use tokio::io::{AsyncRead, AsyncWrite};

use crate::async_trait;

cfg_feature! {
    #![feature = "acme"]
    pub mod acme;
    pub use acme::AcmeListener;
}
cfg_feature! {
    #![feature = "native-tls"]
    pub mod native_tls;
    pub use native_tls::NativeTlsListener;
}
cfg_feature! {
    #![feature = "rustls"]
    pub mod rustls;
    pub use rustls::RustlsListener;
}
cfg_feature! {
    #![feature = "openssl"]
    pub mod openssl;
    pub use self::openssl::OpensslListener;
}
cfg_feature! {
    #![unix]
    pub mod unix;
}
mod addr;
pub use addr::SocketAddr;

mod tcp;
pub use tcp::TcpListener;

mod joined;
pub use joined::JoinedListener;

cfg_feature! {
    #![unix]
    pub use unix::UnixListener;
}

cfg_feature! {
    #![any(feature = "native-tls", feature = "rustls", feature = "openssl", feature = "acme")]
    mod handshake_stream;
    pub use handshake_stream::HandshakeStream;
}

/// Acceptor's return type.
pub struct Accepted<S> {
    /// Incoming stream.
    pub stream: S,
    /// Local addr.
    pub local_addr: SocketAddr,
    /// Remote addr.
    pub remote_addr: SocketAddr,
}

/// Acceptor trait.
#[async_trait]
pub trait Acceptor: Send {
    /// Conn type
    type Conn: AsyncRead + AsyncWrite + Send + Unpin + 'static;
    /// Error type
    type Error: std::error::Error + Send + Sync + 'static;

    /// Returns the local address that this listener is bound to.
    fn local_addrs(&self) -> Vec<&SocketAddr>;

    /// Accepts a new incoming connection from this listener.
    async fn accept(&mut self) -> Result<Accepted<Self::Conn>, Self::Error>;
}

/// Listener trait
#[async_trait]
pub trait Listener: Acceptor {
    /// Join current Listener with the other.
    #[inline]
    fn join<T>(self, other: T) -> JoinedListener<Self, T>
    where
        Self: Sized,
    {
        JoinedListener::new(self, other)
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

        let Accepted { stream, .. } = listener.accept().await.unwrap();
        assert_eq!(stream.read_i32().await.unwrap(), 150);
    }

    #[tokio::test]
    async fn test_joined_listener() {
        let addr1 = std::net::SocketAddr::from(([127, 0, 0, 1], 6978));
        let addr2 = std::net::SocketAddr::from(([127, 0, 0, 1], 6979));

        let mut listener = TcpListener::bind(addr1).join(TcpListener::bind(addr2));
        tokio::spawn(async move {
            let mut stream = TcpStream::connect(addr1).await.unwrap();
            stream.write_i32(50).await.unwrap();

            let mut stream = TcpStream::connect(addr2).await.unwrap();
            stream.write_i32(100).await.unwrap();
        });
        let Accepted { mut stream, .. } = listener.accept().await.unwrap();
        let first = stream.read_i32().await.unwrap();
        let Accepted { mut stream, .. } = listener.next().await.unwrap();
        let second = stream.read_i32().await.unwrap();
        assert_eq!(first + second, 150);
    }
}
