//! Listener trait and it's implements.
use std::io::Result as IoResult;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::async_trait;

// cfg_feature! {
//     #![feature = "acme"]
//     pub mod acme;
//     pub use acme::AcmeListener;
// }
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
    mod tls_conn_stream;
    pub use tls_conn_stream::TlsConnStream;
}

#[cfg(any(feature = "rustls", feature = "native-tls", feature = "openssl"))]
/// A type that can convert into tls config stream.
pub trait IntoConfigStream<C>: Send + 'static {
    /// TLS config stream.
    type Stream: futures_util::Stream<Item = C> + Send + 'static;

    /// Consume itself and return tls config stream.
    fn into_stream(self) -> Self::Stream;
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

impl<S> Accepted<S> {
    /// Map stream and returns a new `Accepted`.
    #[inline]
    pub fn map_stream<T>(self, wrap_fn: impl FnOnce(S) -> T) -> Accepted<T> {
        let Accepted {
            stream,
            local_addr,
            remote_addr,
        } = self;
        Accepted {
            stream: wrap_fn(stream),
            local_addr,
            remote_addr,
        }
    }
}

/// Acceptor trait.
#[async_trait]
pub trait Acceptor {
    /// Conn type
    type Conn: AsyncRead + AsyncWrite + Send + Unpin + 'static;

    /// Returns the local address that this listener is bound to.
    fn local_addrs(&self) -> Vec<&SocketAddr>;

    /// Accepts a new incoming connection from this listener.
    async fn accept(&mut self) -> IoResult<Accepted<Self::Conn>>;
}

/// Listener trait
#[async_trait]
pub trait Listener {
    /// Acceptor type.
    type Acceptor: Acceptor;
    /// Join current Listener with the other.
    #[inline]
    fn join<T>(self, other: T) -> JoinedListener<Self, T>
    where
        Self: Sized,
    {
        JoinedListener::new(self, other)
    }
    /// Convert into acceptor.
    async fn into_acceptor(self) -> IoResult<Self::Acceptor>;
}

#[derive(Debug)]
pub enum CommProtocol {
    Tcp,
    Udp,
    Unix,
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
