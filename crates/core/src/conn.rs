//! Connection and listener implementations for handling HTTP connections.
//!
//! This module provides the foundational types and traits for accepting and managing
//! network connections in Salvo. It includes:
//!
//! - **Listeners**: Types that bind to addresses and produce acceptors
//! - **Acceptors**: Types that accept incoming connections and produce streams
//! - **TLS Support**: Implementations for [`rustls`], [`native_tls`], and [`openssl`]
//! - **Protocol Support**: HTTP/1, HTTP/2, and HTTP/3 (QUIC) via the [`quinn`] module
//! - **Unix Sockets**: Support for Unix domain sockets on Unix platforms
//!
//! # Architecture
//!
//! The connection system follows a layered architecture:
//!
//! 1. [`Listener`] - Binds to an address and creates an [`Acceptor`]
//! 2. [`Acceptor`] - Accepts incoming connections and returns [`Accepted`] structs
//! 3. [`Coupler`] - Couples the stream with HTTP protocol handling
//!
//! # Basic Usage
//!
//! ```no_run
//! use salvo_core::prelude::*;
//! use salvo_core::conn::TcpListener;
//!
//! #[tokio::main]
//! async fn main() {
//!     let acceptor = TcpListener::new("127.0.0.1:7878").bind().await;
//!     // Use acceptor with Server
//! }
//! ```
//!
//! # TLS Support
//!
//! Multiple TLS backends are available through feature flags:
//!
//! - `rustls` - Pure Rust TLS implementation via [`RustlsListener`]
//! - `native-tls` - Platform-native TLS via [`NativeTlsListener`]
//! - `openssl` - OpenSSL bindings via [`OpensslListener`]
//!
//! # Joining Listeners
//!
//! Multiple listeners can be joined together using the [`JoinedListener`]:
//!
//! ```ignore
//! use salvo_core::conn::{TcpListener, JoinedListener};
//!
//! let listener = TcpListener::new("0.0.0.0:80")
//!     .join(TcpListener::new("0.0.0.0:443"));
//! ```
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
/// A type that can convert into a TLS configuration stream.
///
/// This trait enables dynamic TLS configuration updates at runtime.
/// Implementations can provide a stream of configuration values that
/// will be applied to new connections as they arrive.
///
/// # Use Cases
///
/// - Hot-reloading TLS certificates without server restart
/// - Rotating certificates on a schedule
/// - Loading certificates from external sources (e.g., HashiCorp Vault)
///
/// # Example
///
/// A simple implementation might wrap a static configuration:
///
/// ```ignore
/// use futures_util::stream::{self, StreamExt};
///
/// impl IntoConfigStream<ServerConfig> for MyConfig {
///     type Stream = stream::Once<futures_util::future::Ready<ServerConfig>>;
///
///     fn into_stream(self) -> Self::Stream {
///         stream::once(futures_util::future::ready(self.into_server_config()))
///     }
/// }
/// ```
pub trait IntoConfigStream<C> {
    /// The stream type that yields TLS configurations.
    type Stream: futures_util::Stream<Item = C> + Send + 'static;

    /// Consumes this value and returns a stream of TLS configurations.
    fn into_stream(self) -> Self::Stream;
}

/// Represents an accepted connection from an [`Acceptor`].
///
/// This struct contains all the information needed to handle an incoming connection,
/// including the stream itself, addressing information, and protocol metadata.
///
/// # Type Parameters
///
/// - `C`: The [`Coupler`] type that will handle HTTP protocol negotiation
/// - `S`: The underlying stream type (must be `Send + 'static`)
///
/// # Fields
///
/// The struct provides access to:
/// - The raw connection stream for reading/writing data
/// - Local and remote socket addresses for logging and access control
/// - HTTP scheme (http/https) for proper URL construction
/// - Optional fusewire for connection lifecycle management
pub struct Accepted<C, S>
where
    C: Coupler<Stream = S>,
    S: Send + 'static,
{
    /// The coupler responsible for coupling the stream with HTTP handling.
    ///
    /// The coupler manages the HTTP protocol negotiation and connection lifecycle.
    pub coupler: C,
    /// The underlying I/O stream for this connection.
    ///
    /// This is the raw stream that will be used for reading requests and writing responses.
    pub stream: S,
    /// Optional fusewire for connection protection and monitoring.
    ///
    /// When set, this allows the connection to be monitored for slow HTTP attacks
    /// and other malicious behavior patterns.
    pub fusewire: Option<ArcFusewire>,
    /// The local address this connection was accepted on.
    ///
    /// Useful for multi-homed servers or logging purposes.
    pub local_addr: SocketAddr,
    /// The remote address of the connecting client.
    ///
    /// Can be used for access control, logging, or rate limiting.
    pub remote_addr: SocketAddr,
    /// The HTTP scheme for this connection (http or https).
    ///
    /// Used to construct proper URLs and determine if the connection is secure.
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

/// A trait for types that can accept incoming network connections.
///
/// Acceptors are created by [`Listener::bind()`] and are responsible for
/// accepting new connections and returning them as [`Accepted`] structs.
///
/// # Associated Types
///
/// - `Coupler`: The type that handles HTTP protocol negotiation for accepted connections
/// - `Stream`: The underlying I/O stream type for connections
///
/// # Implementation Notes
///
/// Implementations should handle connection acceptance asynchronously and
/// return proper I/O errors when connections fail.
///
/// # Example
///
/// Using an acceptor with a server:
///
/// ```ignore
/// use salvo_core::conn::{TcpListener, Listener, Acceptor};
///
/// let mut acceptor = TcpListener::new("127.0.0.1:8080").bind().await;
///
/// // Get information about bound addresses
/// for holding in acceptor.holdings() {
///     println!("Listening on {}", holding);
/// }
/// ```
pub trait Acceptor: Send {
    /// The coupler type used for HTTP protocol handling.
    type Coupler: Coupler<Stream = Self::Stream> + Unpin + Send + 'static;
    /// The underlying stream type for accepted connections.
    type Stream: Unpin + Send + 'static;

    /// Returns the holding information for all addresses this acceptor is bound to.
    ///
    /// The returned slice contains [`Holding`] structs with information about
    /// each bound address, supported HTTP versions, and scheme.
    fn holdings(&self) -> &[Holding];

    /// Accepts the next incoming connection.
    ///
    /// This method waits for a new connection and returns an [`Accepted`] struct
    /// containing the connection stream and metadata.
    ///
    /// # Parameters
    ///
    /// - `fuse_factory`: Optional factory for creating fusewires to protect against
    ///   slow HTTP attacks and other malicious patterns
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the connection cannot be accepted.
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

/// Information about a bound listener address.
///
/// This struct contains metadata about an address that a listener is bound to,
/// including the socket address, supported HTTP versions, and the HTTP scheme.
///
/// # Display Format
///
/// When displayed, it shows in the format: `[HTTP/1.1, HTTP/2] on https://127.0.0.1:443`
#[derive(Clone, Debug)]
pub struct Holding {
    /// The local socket address the listener is bound to.
    pub local_addr: SocketAddr,
    /// The HTTP versions supported on this address (e.g., HTTP/1.1, HTTP/2).
    pub http_versions: Vec<Version>,
    /// The HTTP scheme for connections on this address (http or https).
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
/// A trait for coupling streams with HTTP protocol handling.
///
/// The coupler is responsible for taking a raw stream and connecting it with
/// the HTTP protocol layer. It handles protocol negotiation (HTTP/1.1 vs HTTP/2)
/// and manages the connection lifecycle.
///
/// # Purpose
///
/// Different connection types require different handling:
/// - Plain TCP connections use a straightforward coupler
/// - TLS connections may need ALPN negotiation for HTTP/2
/// - QUIC connections have their own protocol handling
///
/// # Associated Types
///
/// - `Stream`: The type of stream this coupler works with
pub trait Coupler: Send {
    /// The connection stream type this coupler handles.
    type Stream: Send + 'static;

    /// Couples the stream with HTTP protocol handling.
    ///
    /// This method takes ownership of the stream and handles it according
    /// to the configured HTTP builder settings.
    ///
    /// # Parameters
    ///
    /// - `stream`: The raw I/O stream to handle
    /// - `handler`: The hyper handler that processes HTTP requests
    /// - `builder`: Configuration for HTTP/1 and HTTP/2 behavior
    /// - `graceful_stop_token`: Optional token for graceful shutdown signaling
    ///
    /// # Returns
    ///
    /// A future that completes when the connection is closed or an error occurs.
    fn couple(
        &self,
        stream: Self::Stream,
        handler: HyperHandler,
        builder: Arc<HttpBuilder>,
        graceful_stop_token: Option<CancellationToken>,
    ) -> BoxFuture<'static, IoResult<()>>;
}

/// A trait for types that can bind to an address and create an acceptor.
///
/// Listeners are the starting point for accepting connections. They encapsulate
/// the address binding logic and produce an [`Acceptor`] that can accept connections.
///
/// # Basic Usage
///
/// ```no_run
/// use salvo_core::conn::{TcpListener, Listener};
///
/// # async fn example() {
/// let acceptor = TcpListener::new("127.0.0.1:8080").bind().await;
/// # }
/// ```
///
/// # Error Handling
///
/// Use [`try_bind()`](Listener::try_bind) instead of [`bind()`](Listener::bind)
/// when you need to handle binding errors gracefully:
///
/// ```no_run
/// use salvo_core::conn::{TcpListener, Listener};
///
/// # async fn example() -> salvo_core::Result<()> {
/// let acceptor = TcpListener::new("127.0.0.1:8080").try_bind().await?;
/// # Ok(())
/// # }
/// ```
///
/// # Combining Listeners
///
/// Multiple listeners can be combined using the [`join()`](Listener::join) method:
///
/// ```ignore
/// let combined = TcpListener::new("0.0.0.0:80")
///     .join(TcpListener::new("0.0.0.0:443"));
/// ```
pub trait Listener: Send {
    /// The type of acceptor this listener produces.
    type Acceptor: Acceptor;

    /// Binds to the configured address and returns an acceptor.
    ///
    /// # Panics
    ///
    /// Panics if binding fails. Use [`try_bind()`](Listener::try_bind) for
    /// fallible binding.
    fn bind(self) -> impl Future<Output = Self::Acceptor> + Send
    where
        Self: Sized + Send + 'static,
    {
        async move { self.try_bind().await.expect("bind failed") }.boxed()
    }

    /// Attempts to bind to the configured address.
    ///
    /// # Errors
    ///
    /// Returns an error if the address cannot be bound (e.g., already in use,
    /// permission denied, or invalid address).
    fn try_bind(self) -> impl Future<Output = crate::Result<Self::Acceptor>> + Send;

    /// Joins this listener with another, creating a combined listener.
    ///
    /// The resulting [`JoinedListener`] will accept connections from both
    /// listeners simultaneously.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let listener = http_listener.join(https_listener);
    /// ```
    #[inline]
    fn join<T>(self, other: T) -> JoinedListener<Self, T>
    where
        Self: Sized + Send,
    {
        JoinedListener::new(self, other)
    }
}

/// A type-erased async stream for dynamic acceptors.
///
/// `DynStream` wraps any async read/write stream as a boxed trait object,
/// allowing different stream types to be used uniformly. This is useful
/// when you need to handle multiple stream types through a single interface.
///
/// # Implementation Details
///
/// Internally, the stream is split into separate reader and writer halves,
/// which are stored as boxed trait objects. This allows the stream to be
/// used with any underlying type that implements [`AsyncRead`] and [`AsyncWrite`].
///
/// # Performance Note
///
/// Using `DynStream` incurs the cost of dynamic dispatch. For performance-critical
/// applications, prefer using concrete stream types when possible.
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
