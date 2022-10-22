//! Listener trait and it's implements.
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::http::version::{Version, HttpConnection};
use crate::{async_trait, handler};

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
    #![feature = "http3"]
    pub(crate) mod http3;
    pub mod quic;
    pub use self::quic::QuicListener;
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

mod proto;
pub(crate) use proto::HttpBuilders;

use self::quic::H3Connection;

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
        } = self;
        Accepted {
            conn: wrap_fn(conn),
            local_addr,
            remote_addr,
        }
    }
}

// impl<C> Accepted<C>
// where
//     C: HttpConnection + AsyncRead + AsyncWrite + Unpin + Send + 'static,
// {
//     #[inline]
//     pub(crate) async fn serve_connection(self, service: Arc<crate::Service>, builders: Arc<HttpBuilders>) -> IoResult<()> {
//         let Self {
//             mut conn,
//             local_addr,
//             remote_addr,
//         } = self;
//         let handler = service.hyper_handler(local_addr, remote_addr);
//         let version = match conn.http_version().await {
//             Some(version) => version,
//             None => return Err(IoError::new(ErrorKind::Other, "http version not detected")),
//         };
//         match version {
//             #[cfg(feature = "http1")]
//             Version::HTTP_10 | Version::HTTP_11 => builders
//                 .http1
//                 .serve_connection(conn, handler)
//                 .with_upgrades()
//                 .await
//                 .map_err(|e| IoError::new(ErrorKind::Other, e.to_string())),
//             #[cfg(feature = "http2")]
//             Version::HTTP_2 => builders
//                 .http2
//                 .serve_connection(conn, handler)
//                 .await
//                 .map_err(|e| IoError::new(ErrorKind::Other, e.to_string())),
//             #[cfg(feature = "http3")]
//             Version::HTTP_3 => builders.http3.serve_connection(conn, handler).await,
//         }
//     }
// }

/// Acceptor trait.
#[async_trait]
pub trait Acceptor {
    /// Conn type
    type Conn: HttpConnection + AsyncRead + AsyncWrite + Send + Unpin + 'static;

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
