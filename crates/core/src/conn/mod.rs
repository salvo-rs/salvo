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

use futures_util::Stream;
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
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    pub coupler: C,
    /// Incoming stream.
    pub stream: S,
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
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
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
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    #[inline]
    pub fn map_into<TC, TS>(
        self,
        coupler_fn: impl FnOnce(C) -> TC,
        stream_fn: impl FnOnce(S) -> TS,
    ) -> Accepted<TC, TS>
    where
        TC: Coupler<Stream = TS>,
        TS: AsyncRead + AsyncWrite + Unpin + Send + 'static,
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
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    /// Returns the holding information that this listener is bound to.
    fn holdings(&self) -> &[Holding];

    /// Accepts a new incoming connection from this listener.
    fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> impl Future<Output = IoResult<Accepted<Self::Coupler, Self::Stream>>> + Send;
}

// pub trait DynAcceptor: Send {
//     fn holdings(&self) -> &[Holding];

//     /// Accepts a new incoming connection from this listener.
//     fn accept(
//         &mut self,
//         fuse_factory: Option<ArcFuseFactory>,
//     ) -> BoxFuture<'_, IoResult<Accepted<Box<dyn DynCoupler>, DynStream>>>;
// }
// impl Acceptor for dyn DynAcceptor + '_ {
//     type Coupler = Box<dyn DynCoupler>;
//     type Stream = DynStream;

//     fn holdings(&self) -> &[Holding] {
//         DynAcceptor::holdings(self)
//     }

//     /// Accepts a new incoming connection from this listener.
//     async fn accept(
//         &mut self,
//         fuse_factory: Option<ArcFuseFactory>,
//     ) -> IoResult<Accepted<Self::Coupler, Self::Stream>> {
//         DynAcceptor::accept(self, fuse_factory).await
//     }
// }

// pub struct ToDynAcceptor<A>(pub A);
// impl<A: Acceptor> DynAcceptor for ToDynAcceptor<A> {
//     fn holdings(&self) -> &[Holding] {
//         self.0.holdings()
//     }

//     /// Accepts a new incoming connection from this listener.
//     fn accept(
//         &mut self,
//         fuse_factory: Option<ArcFuseFactory>,
//     ) -> BoxFuture<'_, IoResult<Accepted<Box<dyn DynCoupler>, DynStream>>> {
//         async move {
//             let accepted = self.0.accept(fuse_factory).await?;
//             Ok(accepted.map_into(
//                 |c| {
//                     let conn: Box<dyn DynCoupler> = Box::new(ToDynCoupler(c));
//                     conn
//                 },
//                 DynStream::new,
//             ))
//         }
//         .boxed()
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
    type Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static;

    /// Couple http connection.
    fn couple(
        &self,
        stream: Self::Stream,
        handler: HyperHandler,
        builder: Arc<HttpBuilder>,
        graceful_stop_token: Option<CancellationToken>,
    ) -> impl Future<Output = IoResult<()>> + Send;
}

// pub trait DynCoupler: Send {
//     fn couple<S>(
//         &self,
//         stream: S,
//         handler: HyperHandler,
//         builder: Arc<HttpBuilder>,
//         graceful_stop_token: Option<CancellationToken>,
//     ) -> BoxFuture<'static, IoResult<()>>
//     where
//         S: AsyncRead + AsyncWrite + Unpin + Send + 'static;
// }

// impl Coupler for dyn DynCoupler + '_ {
//     type Stream = DynStream;

//     async fn couple(
//         &self,
//         stream: Self::Stream,
//         handler: HyperHandler,
//         builder: Arc<HttpBuilder>,
//         graceful_stop_token: Option<CancellationToken>,
//     ) -> IoResult<()> {
//         DynCoupler::couple(
//             self,
//             DynStream::new(stream),
//             handler,
//             builder,
//             graceful_stop_token,
//         )
//         .await
//     }
// }

// impl Coupler for Box<dyn DynCoupler> {
//     type Stream = DynStream;

//     async fn couple(
//         &self,
//         stream: Self::Stream,
//         handler: HyperHandler,
//         builder: Arc<HttpBuilder>,
//         graceful_stop_token: Option<CancellationToken>,
//     ) -> IoResult<()> {
//         (*self)
//             .couple(
//                 DynStream::new(stream),
//                 handler,
//                 builder,
//                 graceful_stop_token,
//             )
//             .await
//     }
// }

// pub struct ToDynCoupler<C>(pub C);
// impl<C: Coupler + 'static> DynCoupler for ToDynCoupler<C> {
//     fn couple(
//         &self,
//         stream: C::Stream,
//         handler: HyperHandler,
//         builder: Arc<HttpBuilder>,
//         graceful_stop_token: Option<CancellationToken>,
//     ) -> BoxFuture<'static, IoResult<()>> {
//         async move {
//             self.0
//                 .couple(stream, handler, builder, graceful_stop_token)
//                 .await
//         }
//         .boxed()
//     }
// }

// /// Get Http version from alpha.
// pub fn version_from_alpn(proto: impl AsRef<[u8]>) -> Version {
//     if proto.as_ref().windows(2).any(|window| window == b"h2") {
//         Version::HTTP_2
//     } else {
//         Version::HTTP_11
//     }
// }

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

    // fn boxed(self) -> Box<dyn DynListener>
    // where
    //     Self: Sized + Send + 'static,
    //     Self::Acceptor: Acceptor + Unpin + 'static,
    // {
    //     Box::new(ToDynListener(self))
    // }
}

// pub trait DynListener: Send {
//     fn try_bind(self) -> BoxFuture<'static, crate::Result<Box<dyn DynAcceptor>>>;
// }
// // impl DynListener for Pin<Box<dyn DynListener + '_>> {
// //     fn try_bind(self) -> BoxFuture<'static, crate::Result<Box<dyn DynAcceptor>>> {
// //         DynListener::try_bind(self)
// //     }
// // }
// impl Listener for Pin<Box<dyn DynListener + '_>> {
//     type Acceptor = Box<dyn DynAcceptor>;

//     fn try_bind(self) -> BoxFuture<'static, crate::Result<Self::Acceptor>> {
//         DynListener::try_bind(self)
//     }
// }

// pub struct ToDynListener<L>(pub L);
// impl<L> ToDynListener<L>
// where
//     L: Listener + Unpin + 'static,
//     L::Acceptor: Acceptor + Unpin + 'static,
// {
//     pub fn join_boxed<T>(self, other: T) -> Box<dyn DynListener>
//     where
//         Self: Sized + Send,
//         T: Listener + Unpin + 'static,
//         T::Acceptor: Acceptor + Unpin + 'static,
//     {
//         Box::new(ToDynListener(JoinedListener::new(self, other)))
//     }
// }
// impl<L: Listener + 'static> DynListener for ToDynListener<L> {
//     fn try_bind(self) -> BoxFuture<'static, crate::Result<Box<dyn DynAcceptor>>> {
//         async move {
//             let acceptor: Box<dyn DynAcceptor> = Box::new(ToDynAcceptor(self.0.try_bind().await?));
//             Ok(acceptor)
//         }
//         .boxed()
//     }
// }

// impl<L: Listener + 'static> Listener for ToDynListener<L> {
//     type Acceptor = L::Acceptor;

//     fn try_bind(self) -> BoxFuture<'static, crate::Result<Self::Acceptor>> {
//         self.0.try_bind()
//     }
// }

/// A dynmaic stream type.
pub struct DynStream {
    reader: Box<dyn AsyncRead + Send + Unpin + 'static>,
    writer: Box<dyn AsyncWrite + Send + Unpin + 'static>,
}

impl DynStream {
    fn new(stream: impl AsyncRead + AsyncWrite + Send + Unpin + 'static) -> Self {
        let (reader, writer) = tokio::io::split(stream);
        Self {
            reader: Box::new(reader),
            writer: Box::new(writer),
        }
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
