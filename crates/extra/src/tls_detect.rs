//! TLS-detecting listener that serves both HTTP and HTTPS on the same port.
//!
//! This module provides [`TlsDetectListener`] which inspects the first byte of
//! each incoming connection to determine whether the client is speaking TLS or
//! plain HTTP. Both connection types are served normally — the listener **does not
//! perform any redirect**. It only sets the HTTP scheme (`HTTPS` or `HTTP`) so that
//! downstream handlers and middleware can distinguish them.
//!
//! # When to Use
//!
//! Use this when you can only expose a single port (e.g., behind a firewall,
//! Docker container, or reverse proxy) but still want to accept both HTTP and
//! HTTPS connections.
//!
//! # Example — serving both protocols, no redirect
//!
//! ```ignore
//! use salvo_core::conn::rustls::{Keycert, RustlsConfig};
//! use salvo_core::prelude::*;
//! use salvo_extra::tls_detect::TlsDetectListener;
//!
//! #[handler]
//! async fn hello() -> &'static str {
//!     "Hello World"
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::new().get(hello);
//!     let service = Service::new(router);
//!
//!     let tls_config = RustlsConfig::new(
//!         Keycert::new()
//!             .cert(include_bytes!("../certs/cert.pem").as_ref())
//!             .key(include_bytes!("../certs/key.pem").as_ref()),
//!     );
//!
//!     let acceptor = TlsDetectListener::new("0.0.0.0:8443", tls_config)
//!         .try_bind()
//!         .await
//!         .expect("failed to bind tls-detect listener");
//!     Server::new(acceptor).serve(service).await;
//! }
//! ```
//!
//! # Example — redirect HTTP to HTTPS (opt-in)
//!
//! Add [`ForceHttps`](super::force_https::ForceHttps) middleware if you want
//! plain HTTP requests to receive a 301 redirect to HTTPS:
//!
//! ```ignore
//! let service = Service::new(router).hoop(
//!     salvo_extra::force_https::ForceHttps::new()
//! );
//! ```
//!
//! # How It Works
//!
//! 1. A raw TCP connection is accepted.
//! 2. The first byte is peeked (not consumed) within a timeout:
//!    - `0x16` → TLS ClientHello → perform TLS handshake, set scheme to `HTTPS`
//!    - Otherwise → plain HTTP, set scheme to `HTTP`
//! 3. Both types are passed to the handler chain. Redirect behavior is entirely
//!    controlled by your middleware (e.g., `ForceHttps`), not by this listener.

use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use http::uri::Scheme;
use salvo_core::conn::rustls::{RustlsConfig, ServerConfig};
use salvo_core::conn::tcp::TcpCoupler;
use salvo_core::conn::{
    Accepted, Acceptor, CancellationToken, ConnCtrl, HandshakeStream, Holding, HttpBuilder,
    HyperHandler, Listener, StraightStream,
};
use salvo_core::fuse::{ArcFusePolicy, FuseAction, FuseInfo, TransProto};
use salvo_core::http::Version;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener as TokioTcpListener, TcpStream, ToSocketAddrs};
use tokio_rustls::server::TlsStream;

// ---------------------------------------------------------------------------
// Stream types
// ---------------------------------------------------------------------------

/// The plain (non-TLS) half of a detected connection.
type PlainStream = StraightStream<TcpStream>;

/// The TLS half of a detected connection (boxed to reduce enum size disparity).
type TlsStreamInner = HandshakeStream<TlsStream<PlainStream>>;

/// An enum stream that holds either a TLS or plain connection.
pub enum DetectStream {
    /// TLS connection — handshake completed lazily on first I/O.
    Tls(Box<TlsStreamInner>),
    /// Plain HTTP connection.
    Plain(PlainStream),
}

impl Debug for DetectStream {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("DetectStream").finish()
    }
}

impl AsyncRead for DetectStream {
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<IoResult<()>> {
        match self.get_mut() {
            Self::Tls(s) => Pin::new(s).poll_read(cx, buf),
            Self::Plain(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for DetectStream {
    #[inline]
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<IoResult<usize>> {
        match self.get_mut() {
            Self::Tls(s) => Pin::new(s).poll_write(cx, buf),
            Self::Plain(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        match self.get_mut() {
            Self::Tls(s) => Pin::new(s).poll_flush(cx),
            Self::Plain(s) => Pin::new(s).poll_flush(cx),
        }
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<IoResult<()>> {
        match self.get_mut() {
            Self::Tls(s) => Pin::new(s).poll_shutdown(cx),
            Self::Plain(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

// ---------------------------------------------------------------------------
// Coupler
// ---------------------------------------------------------------------------

/// Coupler for TLS-detected connections.
///
/// Delegates to [`TcpCoupler`] for each stream variant independently,
/// since both implement `AsyncRead + AsyncWrite + Unpin + Send`.
#[derive(Debug)]
pub struct DetectCoupler;

impl salvo_core::conn::Coupler for DetectCoupler {
    type Stream = DetectStream;

    fn couple(
        &self,
        stream: Self::Stream,
        handler: HyperHandler,
        builder: Arc<HttpBuilder>,
        graceful_stop_token: Option<CancellationToken>,
    ) -> BoxFuture<'static, IoResult<()>> {
        match stream {
            DetectStream::Tls(s) => {
                TcpCoupler::<TlsStreamInner>::new()
                    .couple(*s, handler, builder, graceful_stop_token)
                    .boxed()
            }
            DetectStream::Plain(s) => {
                TcpCoupler::<PlainStream>::new()
                    .couple(s, handler, builder, graceful_stop_token)
                    .boxed()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Listener
// ---------------------------------------------------------------------------

/// A listener that detects TLS vs plain HTTP on the same port.
///
/// See [module-level documentation](self) for details and examples.
pub struct TlsDetectListener<T> {
    local_addr: T,
    tls_config: RustlsConfig,
}

impl<T: Debug> Debug for TlsDetectListener<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsDetectListener")
            .field("local_addr", &self.local_addr)
            .finish()
    }
}

impl<T> TlsDetectListener<T>
where
    T: ToSocketAddrs + Send + 'static,
{
    /// Create a new `TlsDetectListener` bound to `local_addr` with the given TLS config.
    pub fn new(local_addr: T, tls_config: RustlsConfig) -> Self {
        Self {
            local_addr,
            tls_config,
        }
    }
}

impl<T> Listener for TlsDetectListener<T>
where
    T: ToSocketAddrs + Send + 'static,
{
    type Acceptor = TlsDetectAcceptor;

    async fn try_bind(self) -> salvo_core::Result<Self::Acceptor> {
        let listener = TokioTcpListener::bind(self.local_addr).await?;
        let local_addr = listener.local_addr()?;

        let server_config: ServerConfig = self.tls_config.try_into()?;
        let tls_acceptor = Arc::new(tokio_rustls::TlsAcceptor::from(Arc::new(server_config)));

        // NOTE: holdings reports HTTPS as the primary scheme, but per-connection
        // detection sets the accurate scheme in Accepted.http_scheme (HTTP or HTTPS).
        let holdings = vec![Holding {
            local_addr: local_addr.into(),
            http_versions: vec![Version::HTTP_11, Version::HTTP_2],
            http_scheme: Scheme::HTTPS,
        }];

        tracing::info!("tls-detect listener bound on {local_addr}");

        Ok(TlsDetectAcceptor {
            listener,
            tls_acceptor,
            holdings,
            pending: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Acceptor
// ---------------------------------------------------------------------------

/// An acceptor that detects TLS vs plain HTTP per connection.
///
/// Created by [`TlsDetectListener::bind()`]. You should not construct this directly.
pub struct TlsDetectAcceptor {
    listener: TokioTcpListener,
    tls_acceptor: Arc<tokio_rustls::TlsAcceptor>,
    holdings: Vec<Holding>,
    pending: Option<(TcpStream, SocketAddr)>,
}

impl Debug for TlsDetectAcceptor {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsDetectAcceptor").finish()
    }
}

/// TLS ClientHello records start with `0x16` (Content Type: Handshake).
const TLS_HANDSHAKE_BYTE: u8 = 0x16;

/// Timeout for peeking the first byte of a connection.
/// Prevents slowloris attacks where clients hold connections open without sending data.
const PEEK_TIMEOUT: Duration = Duration::from_secs(5);

impl Acceptor for TlsDetectAcceptor {
    type Coupler = DetectCoupler;
    type Stream = DetectStream;

    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    async fn accept(
        &mut self,
        fuse_policy: Option<ArcFusePolicy>,
    ) -> IoResult<Accepted<Self::Coupler, Self::Stream>> {
        loop {
            if self.pending.is_none() {
                self.pending = Some(self.listener.accept().await?);
            }
            let remote_addr = self.pending.as_ref().expect("pending set above").1;
            let local_addr = self.holdings[0].local_addr.clone();
            let conn_ctrl = ConnCtrl::new();
            let (fuse_config, observer) = match &fuse_policy {
                Some(policy) => {
                    let info = FuseInfo {
                        trans_proto: TransProto::Tcp,
                        remote_addr: remote_addr.into(),
                        local_addr: local_addr.clone(),
                    };
                    match policy.decide(&info).await {
                        FuseAction::Accept(config) => {
                            (Some(config), policy.observe(&info, &conn_ctrl))
                        }
                        FuseAction::Reject => {
                            self.pending = None;
                            continue;
                        }
                    }
                }
                None => (None, None),
            };
            let tcp_stream = &self.pending.as_ref().expect("pending set above").0;

            // Set TCP_NODELAY to minimize latency during TLS handshake.
            let _ = tcp_stream.set_nodelay(true);

            // Peek the first byte without consuming it, with a timeout to prevent
            // slowloris-style connection-hold attacks.
            let mut buf = [0u8; 1];
            let peek_result = tokio::time::timeout(PEEK_TIMEOUT, tcp_stream.peek(&mut buf)).await;

            match peek_result {
                Ok(Ok(n)) if n > 0 => {}
                Ok(Ok(_)) => {
                    // EOF (n == 0) — client closed without sending data.
                    self.pending = None;
                    return Err(IoError::new(ErrorKind::UnexpectedEof, "connection closed before first byte"));
                }
                Ok(Err(e)) => {
                    self.pending = None;
                    return Err(e);
                }
                Err(_) => {
                    // Timeout — client did not send data within the deadline.
                    tracing::debug!("tls-detect: peek timeout from {remote_addr}");
                    self.pending = None;
                    return Err(IoError::new(ErrorKind::TimedOut, "peek timeout"));
                }
            }

            let (tcp_stream, remote_addr) = self.pending.take().expect("pending set above");

            if buf[0] == TLS_HANDSHAKE_BYTE {
                // TLS ClientHello detected.
                tracing::debug!("tls-detect: TLS handshake detected from {remote_addr}");
                let plain_stream =
                    StraightStream::new(tcp_stream, fuse_config, conn_ctrl.clone(), observer);
                let tls_stream = HandshakeStream::new(
                    self.tls_acceptor.accept(plain_stream),
                    fuse_config,
                );

                return Ok(Accepted {
                    coupler: DetectCoupler,
                    stream: DetectStream::Tls(Box::new(tls_stream)),
                    fuse_config,
                    conn_ctrl,
                    local_addr,
                    remote_addr: remote_addr.into(),
                    http_scheme: Scheme::HTTPS,
                });
            } else {
                // Plain HTTP — serve as-is so ForceHttps middleware can redirect.
                let plain_stream =
                    StraightStream::new(tcp_stream, fuse_config, conn_ctrl.clone(), observer);

                return Ok(Accepted {
                    coupler: DetectCoupler,
                    stream: DetectStream::Plain(plain_stream),
                    fuse_config,
                    conn_ctrl,
                    local_addr,
                    remote_addr: remote_addr.into(),
                    http_scheme: Scheme::HTTP,
                });
            }
        }
    }
}
