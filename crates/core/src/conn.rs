//! Various listener implementations for handling HTTP connections.
//!
//! These listeners include implementations for different TLS libraries such as `rustls`, `native-tls`, and `openssl`.
//! The module also provides support for HTTP versions 1 and 2, as well as the QUIC protocol.
//! Additionally, it includes implementations for Unix domain sockets.
use std::fmt::{self, Debug, Display, Formatter};
use std::io::Result as IoResult;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::future::{BoxFuture, FutureExt};
use http::uri::Scheme;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::sync::CancellationToken;

use crate::fuse::{ArcFuseFactory, ArcFusewire};
use crate::http::Version;
use crate::service::HyperHandler;

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
    pub use self::quinn::{QuinnListener, QuinnConnection};
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
pub struct Accepted<C, S>
where
    C: Coupler<Stream = S>,
    S: Send + 'static,
{
    /// Coupler for couple stream.
    pub coupler: C,
    /// Incoming stream.
    pub stream: S,
    /// Fusewire for the connection.
    pub fusewire: Option<ArcFusewire>,
    /// Local addr.
    pub local_addr: SocketAddr,
    /// Remote addr.
    pub remote_addr: SocketAddr,
    /// HTTP scheme.
    pub http_scheme: Scheme,
}
impl<C, S> Debug for Accepted<C, S>
where
    C: Coupler<Stream = S>,
    S: Send + 'static,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Accepted")
            .field("local_addr", &self.local_addr)
            .field("remote_addr", &self.remote_addr)
            .field("http_scheme", &self.http_scheme)
            .finish()
    }
}

impl<C, S> Accepted<C, S>
where
    C: Coupler<Stream = S>,
    S: Send + 'static,
{
    #[inline]
    #[doc(hidden)]
    pub fn map_into<TC, TS>(
        self,
        coupler_fn: impl FnOnce(C) -> TC,
        stream_fn: impl FnOnce(S) -> TS,
    ) -> Accepted<TC, TS>
    where
        TC: Coupler<Stream = TS>,
        TS: Send + 'static,
    {
        let Self {
            coupler,
            stream,
            fusewire,
            local_addr,
            remote_addr,
            http_scheme,
        } = self;
        Accepted {
            coupler: coupler_fn(coupler),
            stream: stream_fn(stream),
            fusewire,
            local_addr,
            remote_addr,
            http_scheme,
        }
    }
}

/// An acceptor that can accept incoming connections.
pub trait Acceptor: Send {
    /// Coupler type.
    type Coupler: Coupler<Stream = Self::Stream> + Unpin + Send + 'static;
    /// Stream type.
    type Stream: Unpin + Send + 'static;

    /// Returns the holding information that this listener is bound to.
    fn holdings(&self) -> &[Holding];

    /// Accepts a new incoming connection from this listener.
    fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> impl Future<Output = IoResult<Accepted<Self::Coupler, Self::Stream>>> + Send;
}

// /// Get Http version from alpha.
// pub fn version_from_alpn(proto: impl AsRef<[u8]>) -> Version {
//     if proto.as_ref().windows(2).any(|window| window == b"h2") {
//         Version::HTTP_2
//     } else {
//         Version::HTTP_11
//     }
// }

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
/// A trait for couple http stream.
pub trait Coupler: Send {
    /// Connection stream type.
    type Stream: Send + 'static;

    /// Couple http connection.
    fn couple(
        &self,
        stream: Self::Stream,
        handler: HyperHandler,
        builder: Arc<HttpBuilder>,
        graceful_stop_token: Option<CancellationToken>,
    ) -> BoxFuture<'static, IoResult<()>>;
}

/// `Listener` represents a listener that can bind to a specific address and port and return an acceptor.
pub trait Listener: Send {
    /// Acceptor type.
    type Acceptor: Acceptor;

    /// Bind and returns acceptor.
    fn bind(self) -> impl Future<Output = Self::Acceptor> + Send
    where
        Self: Sized + Send + 'static,
    {
        async move { self.try_bind().await.expect("bind failed") }.boxed()
    }

    /// Bind and returns acceptor.
    fn try_bind(self) -> impl Future<Output = crate::Result<Self::Acceptor>> + Send;

    /// Join current listener with the other.
    #[inline]
    fn join<T>(self, other: T) -> JoinedListener<Self, T>
    where
        Self: Sized + Send,
    {
        JoinedListener::new(self, other)
    }
}

/// Stream for DynAcceptor.
pub struct DynStream {
    reader: Box<dyn AsyncRead + Send + Unpin + 'static>,
    writer: Box<dyn AsyncWrite + Send + Unpin + 'static>,
}

impl DynStream {
    fn new(stream: impl AsyncRead + AsyncWrite + Send + 'static) -> Self {
        let (reader, writer) = tokio::io::split(stream);
        Self {
            reader: Box::new(reader),
            writer: Box::new(writer),
        }
    }
}

impl Debug for DynStream {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynStream").finish()
    }
}

impl AsyncRead for DynStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<IoResult<()>> {
        let this = &mut *self;
        Pin::new(&mut this.reader).poll_read(cx, buf)
    }
}

impl AsyncWrite for DynStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<IoResult<usize>> {
        let this = &mut *self;
        Pin::new(&mut this.writer).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        let this = &mut *self;
        Pin::new(&mut this.writer).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        let this = &mut *self;
        Pin::new(&mut this.writer).poll_shutdown(cx)
    }
}
