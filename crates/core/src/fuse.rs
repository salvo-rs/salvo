//! Protection mechanisms against slow HTTP attacks and connection abuse.
//!
//! This module provides the "fuse" system, which monitors connections for
//! malicious patterns such as slow HTTP attacks (Slowloris), slow read attacks,
//! and other connection-based denial of service attempts.
//!
//! # Overview
//!
//! The fuse system works by:
//! 1. Creating a [`Fusewire`] for each incoming connection via a [`FuseFactory`]
//! 2. Monitoring connection events (TLS handshake, data read/write, frame handling)
//! 3. "Fusing" (terminating) connections that exhibit suspicious behavior
//!
//! # Key Components
//!
//! - [`FuseFactory`]: Creates fusewires for new connections
//! - [`Fusewire`]: Monitors a single connection for abuse patterns
//! - [`FuseEvent`]: Events reported to fusewires for monitoring
//! - [`FuseInfo`]: Connection metadata provided when creating fusewires
//! - [`FlexFusewire`]: A flexible, configurable fusewire implementation
//!
//! # Example
//!
//! Using the flexible fusewire with custom timeouts:
//!
//! ```ignore
//! use salvo_core::fuse::{FlexFactory, FlexFusewire};
//! use std::time::Duration;
//!
//! let fuse_factory = FlexFactory::new()
//!     .tls_handshake_timeout(Duration::from_secs(10))
//!     .idle_timeout(Duration::from_secs(60));
//! ```
//!
//! # Attack Prevention
//!
//! The fuse system helps protect against:
//!
//! - **Slowloris attacks**: Clients that send HTTP requests very slowly
//! - **Slow read attacks**: Clients that read responses very slowly
//! - **Connection exhaustion**: Keeping many connections open without activity
//! - **TLS negotiation attacks**: Stalling during TLS handshake
//!
//! # Custom Implementations
//!
//! You can implement custom [`FuseFactory`] and [`Fusewire`] traits for
//! specialized monitoring needs, such as integration with external security
//! systems or custom rate limiting logic.

pub mod flex;
use std::sync::Arc;

use async_trait::async_trait;
pub use flex::{FlexFactory, FlexFusewire};

use crate::conn::SocketAddr;

/// The transport protocol used for a connection.
///
/// This enum identifies whether a connection is using TCP (for HTTP/1.1 and HTTP/2)
/// or QUIC (for HTTP/3).
///
/// # Default
///
/// The default transport protocol is [`TransProto::Tcp`].
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransProto {
    /// TCP transport protocol (used for HTTP/1.1 and HTTP/2).
    #[default]
    Tcp,
    /// QUIC transport protocol (used for HTTP/3).
    Quic,
}
impl TransProto {
    /// Returns `true` if this is a TCP connection.
    #[must_use]
    pub fn is_tcp(&self) -> bool {
        matches!(self, Self::Tcp)
    }
    /// Returns `true` if this is a QUIC connection.
    #[must_use]
    pub fn is_quic(&self) -> bool {
        matches!(self, Self::Quic)
    }
}

/// Events reported to a fusewire during connection lifecycle.
///
/// These events allow the fusewire to track connection state and detect
/// potentially malicious behavior patterns such as slow HTTP attacks.
///
/// # Event Flow
///
/// A typical HTTPS connection might produce events in this order:
/// 1. `TlsHandshaking` - TLS negotiation begins
/// 2. `TlsHandshaked` - TLS negotiation completes
/// 3. `WaitFrame` - Waiting for HTTP request
/// 4. `ReadData(n)` - Received n bytes of request data
/// 5. `GainFrame` - Complete HTTP frame received
/// 6. `WriteData(n)` - Sent n bytes of response data
/// 7. `Alive` - Periodic keepalive during idle periods
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FuseEvent {
    /// TLS handshake has started.
    ///
    /// A fusewire may start a timer here to detect stalled TLS negotiations.
    TlsHandshaking,
    /// TLS handshake completed successfully.
    ///
    /// The fusewire should cancel any TLS handshake timeout timer.
    TlsHandshaked,
    /// Connection is alive but idle.
    ///
    /// Sent periodically to indicate the connection is still open but waiting.
    Alive,
    /// Data was read from the connection.
    ///
    /// The `usize` value indicates the number of bytes read.
    ReadData(usize),
    /// Data was written to the connection.
    ///
    /// The `usize` value indicates the number of bytes written.
    WriteData(usize),
    /// Waiting for an HTTP frame (request or continuation).
    ///
    /// A fusewire may start a timer here to detect slow request attacks.
    WaitFrame,
    /// An HTTP frame was received completely.
    ///
    /// The fusewire should reset any frame timeout timers.
    GainFrame,
}

/// Type alias for a thread-safe, shared fuse factory.
pub type ArcFuseFactory = Arc<dyn FuseFactory + Sync + Send + 'static>;
/// Type alias for a thread-safe, shared fusewire.
pub type ArcFusewire = Arc<dyn Fusewire + Sync + Send + 'static>;

/// Information about a connection provided to the fuse factory.
///
/// This struct contains metadata about an incoming connection that can be
/// used to create an appropriate fusewire or make access control decisions.
#[derive(Clone, Debug)]
pub struct FuseInfo {
    /// The transport protocol of the connection (TCP or QUIC).
    pub trans_proto: TransProto,
    /// The remote address of the connecting client.
    pub remote_addr: SocketAddr,
    /// The local address the connection was accepted on.
    pub local_addr: SocketAddr,
}

/// Factory trait for creating fusewires for new connections.
///
/// Implementations of this trait are responsible for creating [`Fusewire`]
/// instances for each incoming connection. The factory pattern allows
/// sharing configuration across all fusewires while creating unique
/// instances for each connection.
///
/// # Example Implementation
///
/// A simple factory using a closure:
///
/// ```ignore
/// use salvo_core::fuse::{FuseFactory, FuseInfo, ArcFusewire};
///
/// let factory = |info: FuseInfo| {
///     println!("New connection from: {}", info.remote_addr);
///     MyCustomFusewire::new(info)
/// };
/// ```
pub trait FuseFactory {
    /// Creates a new fusewire for a connection.
    ///
    /// # Parameters
    ///
    /// - `info`: Information about the new connection
    ///
    /// # Returns
    ///
    /// A thread-safe fusewire instance for monitoring the connection.
    fn create(&self, info: FuseInfo) -> ArcFusewire;
}

/// Trait for monitoring and terminating suspicious connections.
///
/// A fusewire is created for each incoming connection and monitors its
/// behavior throughout its lifecycle. When suspicious activity is detected,
/// the fusewire "fuses" (terminates) the connection.
///
/// # Implementation Notes
///
/// Implementations should:
/// - Track timing between events to detect slowloris-style attacks
/// - Monitor data transfer rates to detect slow read attacks
/// - Maintain connection state to enforce timeouts
///
/// # Example
///
/// ```ignore
/// use salvo_core::fuse::{Fusewire, FuseEvent};
/// use async_trait::async_trait;
///
/// struct TimeoutFusewire {
///     fuse_signal: tokio::sync::Notify,
/// }
///
/// #[async_trait]
/// impl Fusewire for TimeoutFusewire {
///     fn event(&self, event: FuseEvent) {
///         // Reset timeout on activity
///         match event {
///             FuseEvent::ReadData(_) | FuseEvent::WriteData(_) => {
///                 // Reset idle timer
///             }
///             _ => {}
///         }
///     }
///
///     async fn fused(&self) {
///         // Wait until connection should be terminated
///         self.fuse_signal.notified().await;
///     }
/// }
/// ```
#[async_trait]
pub trait Fusewire {
    /// Reports an event from the connection to this fusewire.
    ///
    /// Implementations should use these events to track connection state
    /// and detect suspicious behavior patterns.
    fn event(&self, event: FuseEvent);

    /// Waits until the fusewire determines the connection should be terminated.
    ///
    /// This method is polled by the connection handler. When it returns,
    /// the connection will be forcefully closed.
    async fn fused(&self);
}

impl<T, F> FuseFactory for T
where
    T: Fn(FuseInfo) -> F,
    F: Fusewire + Sync + Send + 'static,
{
    fn create(&self, info: FuseInfo) -> ArcFusewire {
        Arc::new((*self)(info))
    }
}
