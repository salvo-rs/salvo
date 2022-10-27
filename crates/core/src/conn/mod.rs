//! Listener trait and it's implements.
use std::fmt::{self, Display, Formatter};
use std::io::Result as IoResult;

use http::uri::Scheme;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::async_trait;
use crate::http::{HttpConnection, Version};

// cfg_feature! {
//     #![feature = "acme"]
//     pub mod acme;
//     pub use acme::AcmeListener;
// }
cfg_feature! {
    #![feature = "native-tls"]
    pub mod native_tls;
    pub use self::native_tls::NativeTlsListener;
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
    #![feature = "http1"]
    pub use hyper::server::conn::http1;
}
cfg_feature! {
    #![feature = "http2"]
    pub use hyper::server::conn::http2;
}
cfg_feature! {
    #![feature = "http3"]
    pub mod http3;
    pub mod quic;
    pub use self::quic::{QuicListener, H3Connection};
}
cfg_feature! {
    #![unix]
    pub mod unix;
}
pub mod addr;
pub use addr::SocketAddr;

mod tcp;
pub use tcp::TcpListener;

mod joined;
pub use joined::JoinedListener;

mod proto;
pub use proto::HttpBuilders;

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
pub struct Accepted<C> {
    /// Incoming stream.
    pub conn: C,
    /// Local addr.
    pub local_addr: SocketAddr,
    /// Remote addr.
    pub remote_addr: SocketAddr,
    /// Http scheme.
    pub http_scheme: Scheme,
    /// Http version.
    pub http_version: Version,
}

impl<C> Accepted<C>
where
    C: HttpConnection + AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    /// Map connection and returns a new `Accepted`.
    #[inline]
    pub fn map_conn<T>(self, wrap_fn: impl FnOnce(C) -> T) -> Accepted<T> {
        let Accepted {
            conn,
            local_addr,
            remote_addr,
            http_version,
            http_scheme,
        } = self;
        Accepted {
            conn: wrap_fn(conn),
            local_addr,
            remote_addr,
            http_version,
            http_scheme,
        }
    }
}

/// Acceptor trait.
#[async_trait]
pub trait Acceptor {
    /// Conn type
    type Conn: HttpConnection + AsyncRead + AsyncWrite + Send + Unpin + 'static;

    /// Returns the holding information that this listener is bound to.
    fn holdings(&self) -> &[Holding];

    /// Accepts a new incoming connection from this listener.
    async fn accept(&mut self) -> IoResult<Accepted<Self::Conn>>;
}

/// `IntoAcceptor` trait.
#[async_trait]
pub trait IntoAcceptor {
    /// Acceptor type.
    type Acceptor: Acceptor;
    /// Convert into acceptor.
    async fn into_acceptor(self) -> IoResult<Self::Acceptor>;
}

#[async_trait]
impl<T> IntoAcceptor for T
where
    T: Acceptor + Send + 'static,
{
    type Acceptor = T;
    #[inline]
    async fn into_acceptor(self) -> IoResult<Self::Acceptor> {
        Ok(self)
    }
}

/// Holding information.
#[derive(Clone, Debug)]
pub struct Holding {
    /// Local addr.
    pub local_addr: SocketAddr,
    /// Http version.
    pub http_version: Version,
    /// Http scheme.
    pub http_scheme: Scheme,
}
impl Display for Holding {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} on {}://{}",
            self.http_version,
            self.http_scheme,
            self.local_addr.to_string().trim_start_matches("socket://")
        )
    }
}

/// Listener trait
#[async_trait]
pub trait Listener: IntoAcceptor {
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

        let listener = TcpListener::bind(addr);
        let mut acceptor = listener.into_acceptor().await.unwrap();
        tokio::spawn(async move {
            let mut stream = TcpStream::connect(addr).await.unwrap();
            stream.write_i32(150).await.unwrap();
        });

        let Accepted { mut conn, .. } = acceptor.accept().await.unwrap();
        assert_eq!(conn.read_i32().await.unwrap(), 150);
    }

    #[tokio::test]
    async fn test_joined_listener() {
        let addr1 = std::net::SocketAddr::from(([127, 0, 0, 1], 6978));
        let addr2 = std::net::SocketAddr::from(([127, 0, 0, 1], 6979));

        let listener = TcpListener::bind(addr1).join(TcpListener::bind(addr2));
        tokio::spawn(async move {
            let mut stream = TcpStream::connect(addr1).await.unwrap();
            stream.write_i32(50).await.unwrap();

            let mut stream = TcpStream::connect(addr2).await.unwrap();
            stream.write_i32(100).await.unwrap();
        });
        let mut acceptor = listener.into_acceptor().await.unwrap();
        let Accepted { mut conn, .. } = acceptor.accept().await.unwrap();
        let first = conn.read_i32().await.unwrap();
        let Accepted { mut conn, .. } = acceptor.accept().await.unwrap();
        let second = conn.read_i32().await.unwrap();
        assert_eq!(first + second, 150);
    }
}
