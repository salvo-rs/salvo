//! Various listener implementations for handling HTTP connections.
//!
//! These listeners include implementations for different TLS libraries such as `rustls`, `native-tls`, and `openssl`.
//! The module also provides support for HTTP versions 1 and 2, as well as the QUIC protocol.
//! Additionally, it includes implementations for Unix domain sockets.
use std::fmt::{self, Debug, Display, Formatter};
use std::io::Result as IoResult;

use futures_util::future::{BoxFuture, FutureExt};
use http::uri::Scheme;

use crate::fuse::ArcFuseFactory;
use crate::http::{HttpConnection, Version};

mod proto;
pub use proto::HttpBuilder;
mod stream;
pub use stream::*;

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
pub use joined::{JoinedAcceptor, JoinedListener};

cfg_feature! {
    #![unix]
    pub use unix::UnixListener;
}

#[cfg(any(feature = "rustls", feature = "native-tls", feature = "openssl"))]
/// A type that can convert into TLS config stream.
pub trait IntoConfigStream<C> {
    /// TLS config stream.
    type Stream: futures_util::Stream<Item = C> + Send + 'static;

    /// Consume itself and return TLS config stream.
    fn into_stream(self) -> Self::Stream;
}

/// [`Acceptor`]'s return type.
///
/// The `Accepted` struct represents an accepted connection and contains information such as the connection itself,
/// the local and remote addresses, the HTTP scheme, and the HTTP version.
#[non_exhaustive]
pub struct Accepted<C>
where
    C: HttpConnection,
{
    /// Incoming stream.
    pub conn: C,
    /// Local addr.
    pub local_addr: SocketAddr,
    /// Remote addr.
    pub remote_addr: SocketAddr,
    /// HTTP scheme.
    pub http_scheme: Scheme,
}
impl<C> Debug for Accepted<C>
where
    C: HttpConnection,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Accepted")
            .field("local_addr", &self.local_addr)
            .field("remote_addr", &self.remote_addr)
            .field("http_scheme", &self.http_scheme)
            .finish()
    }
}

impl<C> Accepted<C>
where
    C: HttpConnection + Unpin + 'static,
{
    /// Map connection and returns a new `Accepted`.
    #[inline]
    pub fn map_conn<T>(self, wrap_fn: impl FnOnce(C) -> T) -> Accepted<T>
    where
        T: HttpConnection,
    {
        let Self {
            conn,
            local_addr,
            remote_addr,
            http_scheme,
        } = self;
        Accepted {
            conn: wrap_fn(conn),
            local_addr,
            remote_addr,
            http_scheme,
        }
    }
}

/// `Acceptor` represents an acceptor that can accept incoming connections.
pub trait Acceptor: Send {
    /// Conn type
    type Conn: HttpConnection + Send + Unpin + 'static;

    /// Returns the holding information that this listener is bound to.
    fn holdings(&self) -> &[Holding];

    /// Accepts a new incoming connection from this listener.
    fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> impl Future<Output = IoResult<Accepted<Self::Conn>>> + Send;
}

pub trait DynAcceptor: Send {
    fn holdings(&self) -> &[Holding];

    /// Accepts a new incoming connection from this listener.
    fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> BoxFuture<'_, IoResult<Accepted<Box<dyn HttpConnection>>>>;
}
impl DynAcceptor for Box<dyn DynAcceptor + '_> {
    fn holdings(&self) -> &[Holding] {
        (**self).holdings()
    }

    /// Accepts a new incoming connection from this listener.
    fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> BoxFuture<'_, IoResult<Accepted<Box<dyn HttpConnection>>>> {
        (&mut **self).accept(fuse_factory)
    }
}
impl Acceptor for Box<dyn DynAcceptor + '_> {
    type Conn = Box<dyn HttpConnection>;

    fn holdings(&self) -> &[Holding] {
        (**self).holdings()
    }

    /// Accepts a new incoming connection from this listener.
    fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> impl Future<Output = IoResult<Accepted<Self::Conn>>> + Send {
        (&mut **self).accept(fuse_factory)
    }
}

pub struct ToDynAcceptor<A>(pub A);
impl<A: Acceptor> DynAcceptor for ToDynAcceptor<A> {
    fn holdings(&self) -> &[Holding] {
        self.0.holdings()
    }

    /// Accepts a new incoming connection from this listener.
    fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> BoxFuture<'_, IoResult<Accepted<Box<dyn HttpConnection>>>> {
        async move {
            let accepted = self.0.accept(fuse_factory).await?;
            Ok(accepted.map_conn(|c| {
                let conn: Box<dyn HttpConnection> = Box::new(c);
                conn
            }))
        }
        .boxed()
    }
}

/// Holding information.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Holding {
    /// Local address.
    pub local_addr: SocketAddr,
    /// HTTP versions.
    pub http_versions: Vec<Version>,
    /// HTTP scheme.
    pub http_scheme: Scheme,
}
impl Display for Holding {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} on {}://{}",
            self.http_versions,
            self.http_scheme,
            self.local_addr.to_string().trim_start_matches("socket://")
        )
    }
}

/// `Listener` represents a listener that can bind to a specific address and port and return an acceptor.
pub trait Listener: Send {
    /// Acceptor type.
    type Acceptor: Acceptor;

    /// Bind and returns acceptor.
    fn bind(self) -> BoxFuture<'static, Self::Acceptor>
    where
        Self: Sized + Send + 'static,
    {
        async move { self.try_bind().await.expect("bind failed") }.boxed()
    }

    /// Bind and returns acceptor.
    fn try_bind(self) -> BoxFuture<'static, crate::Result<Self::Acceptor>>;

    /// Join current listener with the other.
    #[inline]
    fn join<T>(self, other: T) -> JoinedListener<Self, T>
    where
        Self: Sized + Send,
    {
        JoinedListener::new(self, other)
    }

    fn boxed(self) -> Box<dyn DynListener>
    where
        Self: Sized + Send + 'static,
        Self::Acceptor: Acceptor + Unpin + 'static,
    {
        Box::new(ToDynListener(self))
    }
}

pub trait DynListener: Send {
    fn try_bind(self) -> BoxFuture<'static, crate::Result<Box<dyn DynAcceptor>>>;
}
// impl DynListener for Box<dyn DynListener + '_> {
//     fn try_bind(self) -> BoxFuture<'static, crate::Result<Box<dyn DynAcceptor>>> {
//         (**self).try_bind()
//     }
// }
impl Listener for Box<dyn DynListener + '_> {
    type Acceptor = Box<dyn DynAcceptor>;

    fn try_bind(self) -> BoxFuture<'static, crate::Result<Self::Acceptor>> {
        DynListener::try_bind(self)
    }
}

pub struct ToDynListener<L>(pub L);
impl<L> ToDynListener<L>
where
    L: Listener + Unpin + 'static,
    L::Acceptor: Acceptor + Unpin + 'static,
{
    pub fn join_boxed<T>(self, other: T) -> Box<dyn DynListener>
    where
        Self: Sized + Send,
        T: Listener + Unpin + 'static,
        T::Acceptor: Acceptor + Unpin + 'static,
    {
        Box::new(ToDynListener(JoinedListener::new(self, other)))
    }
}
impl<L: Listener + 'static> DynListener for ToDynListener<L> {
    fn try_bind(self) -> BoxFuture<'static, crate::Result<Box<dyn DynAcceptor>>> {
        async move {
            let acceptor: Box<dyn DynAcceptor> = Box::new(ToDynAcceptor(self.0.try_bind().await?));
            Ok(acceptor)
        }
        .boxed()
    }
}

impl<L: Listener + 'static> Listener for ToDynListener<L> {
    type Acceptor = L::Acceptor;

    fn try_bind(self) -> BoxFuture<'static, crate::Result<Self::Acceptor>> {
        self.0.try_bind()
    }
}
