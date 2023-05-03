//! Listener trait and it's implements.
use std::fmt::{self, Display, Formatter};
use std::io::Result as IoResult;

use http::uri::Scheme;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::async_trait;
use crate::http::{HttpConnection, Version};

cfg_feature! {
    #![feature = "acme"]
    pub mod acme;
    pub use acme::AcmeListener;
}
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
    #![feature = "quinn"]
    pub mod quinn;
    pub use self::quinn::{QuinnListener, H3Connection};
}
cfg_feature! {
    #![unix]
    pub mod unix;
}
pub mod addr;
pub use addr::SocketAddr;

pub mod tcp;
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
    #![any(feature = "rustls", feature = "acme")]
    mod sealed {
        use std::io::{Error as IoError, ErrorKind, Result as IoResult};
        use std::sync::Arc;

        use tokio_rustls::server::TlsStream;
        use tokio::io::{AsyncRead, AsyncWrite};

        use crate::async_trait;
        use crate::service::HyperHandler;
        use crate::http::{version_from_alpn, HttpConnection, Version};
        use crate::conn::HttpBuilders;

        #[cfg(any(feature = "rustls", feature = "acme"))]
        #[async_trait]
        impl<S> HttpConnection for TlsStream<S>
        where
            S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
        {
            async fn version(&mut self) -> Option<Version> {
                self.get_ref().1.alpn_protocol().map(version_from_alpn)
            }
            async fn serve(self, handler: HyperHandler, builders: Arc<HttpBuilders>) -> IoResult<()> {
                #[cfg(not(feature = "http2"))]
                {
                    let _ = handler;
                    let _ = builders;
                    panic!("http2 feature is required");
                }
                #[cfg(feature = "http2")]
                builders
                    .http2
                    .serve_connection(self, handler)
                    .await
                    .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))
            }
        }
    }
}

#[cfg(any(feature = "rustls", feature = "native-tls", feature = "openssl"))]
/// A type that can convert into tls config stream.
pub trait IntoConfigStream<C> {
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
pub trait Listener {
    /// Acceptor type.
    type Acceptor: Acceptor;

    /// Bind and returns acceptor.
    async fn bind(self) -> Self::Acceptor;

    /// Bind and returns acceptor.
    async fn try_bind(self) -> IoResult<Self::Acceptor>;

    /// Join current Listener with the other.
    #[inline]
    fn join<T>(self, other: T) -> JoinedListener<Self, T>
    where
        Self: Sized,
    {
        JoinedListener::new(self, other)
    }
}
