//! A flexible, configurable fusewire implementation.
//!
//! This module provides [`FlexFusewire`] and [`FlexFactory`], which implement
//! the fuse system with configurable timeouts and guard functions.
//!
//! # Features
//!
//! - Configurable TCP idle timeout (default: 30 seconds)
//! - Configurable HTTP frame timeout (default: 60 seconds)
//! - Configurable TLS handshake timeout (default: 10 seconds)
//! - Custom guard functions for additional access control
//!
//! # Example
//!
//! ```ignore
//! use salvo_core::fuse::{FlexFactory, Guard, GuardAction, FuseInfo, FuseEvent};
//! use std::time::Duration;
//!
//! // Create a factory with custom timeouts
//! let factory = FlexFactory::new()
//!     .tcp_idle_timeout(Duration::from_secs(60))
//!     .tcp_frame_timeout(Duration::from_secs(120))
//!     .add_guard(|info: &FuseInfo, event: &FuseEvent| {
//!         // Custom logic to reject connections from certain IPs
//!         GuardAction::ToNext
//!     });
//! ```
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;

use tokio::sync::Notify;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

use super::{ArcFusewire, FuseEvent, FuseFactory, FuseInfo, Fusewire, async_trait};

/// The action returned by a [`Guard`] after checking a connection event.
///
/// Guards inspect connection events and return one of these actions to determine
/// how the fusewire should handle the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GuardAction {
    /// Reject the connection immediately.
    ///
    /// The connection will be terminated as soon as possible.
    Reject,
    /// Pass the event to the next guard in the chain.
    ///
    /// If this is the last guard, the event proceeds normally.
    ToNext,
    /// Permit the event and skip all remaining guards.
    ///
    /// The event proceeds without further guard checks.
    Permit,
}

/// A guard that inspects connection events and decides how to handle them.
///
/// Guards provide a way to add custom access control logic to the fuse system.
/// They can reject suspicious connections, permit trusted ones, or pass events
/// through for standard timeout-based handling.
///
/// # Implementation
///
/// Guards can be implemented as closures:
///
/// ```ignore
/// use salvo_core::fuse::{Guard, GuardAction, FuseInfo, FuseEvent};
///
/// let ip_allowlist = |info: &FuseInfo, _event: &FuseEvent| {
///     if info.remote_addr.to_string().starts_with("10.0.") {
///         GuardAction::Permit  // Trust internal network
///     } else {
///         GuardAction::ToNext  // Apply normal checks
///     }
/// };
/// ```
pub trait Guard: Sync + Send + 'static {
    /// Checks a connection event and returns the appropriate action.
    ///
    /// # Parameters
    ///
    /// - `info`: Information about the connection being checked
    /// - `event`: The event that occurred on the connection
    ///
    /// # Returns
    ///
    /// A [`GuardAction`] indicating how to handle the event.
    fn check(&self, info: &FuseInfo, event: &FuseEvent) -> GuardAction;
}
impl<F> Guard for F
where
    F: Fn(&FuseInfo, &FuseEvent) -> GuardAction + Sync + Send + 'static,
{
    fn check(&self, info: &FuseInfo, event: &FuseEvent) -> GuardAction {
        self(info, event)
    }
}

/// A guard that skips timeout checks for QUIC connections.
///
/// QUIC has its own built-in connection management and timeout handling,
/// so the TCP-oriented timeouts in [`FlexFusewire`] are not applicable.
/// This guard permits all QUIC connections to bypass the fuse checks.
///
/// This guard is included by default in [`FlexFactory::new()`].
///
/// # Example
///
/// ```ignore
/// use salvo_core::fuse::{FlexFactory, skip_quic};
///
/// // skip_quic is included by default
/// let factory = FlexFactory::new();
/// ```
#[must_use]
pub fn skip_quic(info: &FuseInfo, _event: &FuseEvent) -> GuardAction {
    if info.trans_proto.is_quic() {
        GuardAction::Permit
    } else {
        GuardAction::ToNext
    }
}

/// A flexible, configurable fusewire implementation.
///
/// `FlexFusewire` monitors a single connection and terminates it if any of
/// the configured timeouts are exceeded or if a guard rejects the connection.
///
/// # Timeout Behavior
///
/// - **TCP Idle Timeout**: Connection is terminated if no activity occurs within
///   the idle timeout period. Any event resets this timer.
///
/// - **TCP Frame Timeout**: After a `WaitFrame` event, the connection is terminated
///   if a complete frame is not received within the frame timeout period.
///
/// - **TLS Handshake Timeout**: During TLS negotiation, the connection is terminated
///   if the handshake does not complete within the handshake timeout period.
///
/// # Guards
///
/// Guards are checked for every event before timeout handling. They can:
/// - Reject connections immediately
/// - Permit connections to bypass timeout checks
/// - Pass events through for normal handling
///
/// # Creation
///
/// Use [`FlexFactory`] to create `FlexFusewire` instances:
///
/// ```ignore
/// use salvo_core::fuse::{FlexFactory, FuseInfo, TransProto};
/// use salvo_core::conn::SocketAddr;
///
/// let factory = FlexFactory::new();
/// let info = FuseInfo {
///     trans_proto: TransProto::Tcp,
///     remote_addr: "127.0.0.1:8080".parse().unwrap(),
///     local_addr: "0.0.0.0:80".parse().unwrap(),
/// };
/// let fusewire = factory.build(info);
/// ```
pub struct FlexFusewire {
    info: FuseInfo,
    guards: Arc<Vec<Box<dyn Guard>>>,

    reject_token: CancellationToken,

    tcp_idle_timeout: Duration,
    tcp_idle_token: CancellationToken,
    tcp_idle_notify: Arc<Notify>,

    tcp_frame_timeout: Duration,
    tcp_frame_token: CancellationToken,
    tcp_frame_notify: Arc<Notify>,

    tls_handshake_timeout: Duration,
    tls_handshake_token: CancellationToken,
    tls_handshake_notify: Arc<Notify>,
}

impl Debug for FlexFusewire {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("FlexFusewire")
            .field("info", &self.info)
            .field("guards.len", &self.guards.len())
            .field("tcp_idle_timeout", &self.tcp_idle_timeout)
            .field("tcp_frame_timeout", &self.tcp_frame_timeout)
            .field("tls_handshake_timeout", &self.tls_handshake_timeout)
            .finish()
    }
}

impl FlexFusewire {
    /// Create a new `FlexFusewire`.
    #[must_use]
    pub fn new(info: FuseInfo) -> Self {
        Self::builder().build(info)
    }

    /// Create a new `FlexFactory`.
    #[must_use]
    pub fn builder() -> FlexFactory {
        FlexFactory::new()
    }
    /// Get the timeout for close the idle tcp connection.
    #[must_use]
    pub fn tcp_idle_timeout(&self) -> Duration {
        self.tcp_idle_timeout
    }
    /// Get the timeout for close the connection if frame can not be received.
    #[must_use]
    pub fn tcp_frame_timeout(&self) -> Duration {
        self.tcp_frame_timeout
    }
    /// Set the timeout for close the connection if handshake not finished.
    #[must_use]
    pub fn tls_handshake_timeout(&self) -> Duration {
        self.tls_handshake_timeout
    }
}
#[async_trait]
impl Fusewire for FlexFusewire {
    fn event(&self, event: FuseEvent) {
        for guard in self.guards.iter() {
            match guard.check(&self.info, &event) {
                GuardAction::Permit => {
                    return;
                }
                GuardAction::Reject => {
                    self.reject_token.cancel();
                    return;
                }
                GuardAction::ToNext => {}
            }
        }
        self.tcp_idle_notify.notify_waiters();
        match event {
            FuseEvent::TlsHandshaking => {
                let tls_handshake_notify = self.tls_handshake_notify.clone();
                let tls_handshake_timeout = self.tls_handshake_timeout;
                let tls_handshake_token = self.tls_handshake_token.clone();
                tokio::spawn(async move {
                    loop {
                        if tokio::time::timeout(
                            tls_handshake_timeout,
                            tls_handshake_notify.notified(),
                        )
                        .await
                        .is_err()
                        {
                            tls_handshake_token.cancel();
                            break;
                        }
                    }
                });
            }
            FuseEvent::TlsHandshaked => {
                self.tls_handshake_notify.notify_waiters();
            }
            FuseEvent::WaitFrame => {
                let tcp_frame_notify = self.tcp_frame_notify.clone();
                let tcp_frame_timeout = self.tcp_frame_timeout;
                let tcp_frame_token = self.tcp_frame_token.clone();
                tokio::spawn(async move {
                    if tokio::time::timeout(tcp_frame_timeout, tcp_frame_notify.notified())
                        .await
                        .is_err()
                    {
                        tcp_frame_token.cancel();
                    }
                });
            }
            FuseEvent::GainFrame => {
                self.tcp_frame_notify.notify_waiters();
            }
            _ => {}
        }
    }
    async fn fused(&self) {
        tokio::select! {
            _ = self.reject_token.cancelled() => {}
            _ = self.tcp_idle_token.cancelled() => {}
            _ = self.tcp_frame_token.cancelled() => {}
            _ = self.tls_handshake_token.cancelled() => {}
        }
    }
}

/// A factory and builder for creating [`FlexFusewire`] instances.
///
/// `FlexFactory` implements both the builder pattern for configuration and
/// the [`FuseFactory`] trait for creating fusewires.
///
/// # Default Configuration
///
/// | Setting | Default Value |
/// |---------|---------------|
/// | TCP Idle Timeout | 30 seconds |
/// | TCP Frame Timeout | 60 seconds |
/// | TLS Handshake Timeout | 10 seconds |
/// | Guards | [`skip_quic`] only |
///
/// # Example
///
/// ```ignore
/// use salvo_core::fuse::FlexFactory;
/// use std::time::Duration;
///
/// let factory = FlexFactory::new()
///     .tcp_idle_timeout(Duration::from_secs(60))
///     .tcp_frame_timeout(Duration::from_secs(120));
///
/// // Use with Server
/// let server = Server::new(acceptor)
///     .fuse_factory(factory);
/// ```
///
/// # Adding Custom Guards
///
/// ```ignore
/// use salvo_core::fuse::{FlexFactory, GuardAction, FuseInfo, FuseEvent};
///
/// let factory = FlexFactory::new()
///     .add_guard(|info: &FuseInfo, _event: &FuseEvent| {
///         // Custom access control logic
///         GuardAction::ToNext
///     });
/// ```
#[derive(Clone)]
pub struct FlexFactory {
    tcp_idle_timeout: Duration,
    tcp_frame_timeout: Duration,
    tls_handshake_timeout: Duration,

    guards: Arc<Vec<Box<dyn Guard>>>,
}

impl Debug for FlexFactory {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("FlexFactory")
            .field("tcp_idle_timeout", &self.tcp_idle_timeout)
            .field("tcp_frame_timeout", &self.tcp_frame_timeout)
            .field("tls_handshake_timeout", &self.tls_handshake_timeout)
            .field("guards.len", &self.guards.len())
            .finish()
    }
}

impl Default for FlexFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl FlexFactory {
    /// Create a new `FlexFactory`.
    pub fn new() -> Self {
        Self {
            tcp_idle_timeout: Duration::from_secs(30),
            tcp_frame_timeout: Duration::from_secs(60),
            tls_handshake_timeout: Duration::from_secs(10),
            guards: Arc::new(vec![Box::new(skip_quic)]),
        }
    }

    /// Set the timeout for close the idle tcp connection.
    #[must_use]
    pub fn tcp_idle_timeout(mut self, timeout: Duration) -> Self {
        self.tcp_idle_timeout = timeout;
        self
    }
    /// Set the timeout for close the connection if frame can not be received.
    #[must_use]
    pub fn tcp_frame_timeout(mut self, timeout: Duration) -> Self {
        self.tcp_frame_timeout = timeout;
        self
    }

    /// Set guards to new value.
    #[must_use]
    pub fn guards(mut self, guards: Vec<Box<dyn Guard>>) -> Self {
        self.guards = Arc::new(guards);
        self
    }
    /// Add a guard.
    #[must_use]
    pub fn add_guard(mut self, guard: impl Guard) -> Self {
        Arc::get_mut(&mut self.guards)
            .expect("guards get mut failed")
            .push(Box::new(guard));
        self
    }

    /// Build a `FlexFusewire`.
    #[must_use]
    pub fn build(&self, info: FuseInfo) -> FlexFusewire {
        let Self {
            tcp_idle_timeout,
            tcp_frame_timeout,
            tls_handshake_timeout,
            guards,
        } = self.clone();

        let tcp_idle_token = CancellationToken::new();
        let tcp_idle_notify = Arc::new(Notify::new());
        tokio::spawn({
            let tcp_idle_notify = tcp_idle_notify.clone();
            let tcp_idle_token = tcp_idle_token.clone();
            async move {
                loop {
                    if tokio::time::timeout(tcp_idle_timeout, tcp_idle_notify.notified())
                        .await
                        .is_err()
                    {
                        tcp_idle_token.cancel();
                        break;
                    }
                }
            }
        });
        FlexFusewire {
            info,
            guards,

            reject_token: CancellationToken::new(),

            tcp_idle_timeout,
            tcp_idle_token,
            tcp_idle_notify,

            tcp_frame_timeout,
            tcp_frame_token: CancellationToken::new(),
            tcp_frame_notify: Arc::new(Notify::new()),

            tls_handshake_timeout,
            tls_handshake_token: CancellationToken::new(),
            tls_handshake_notify: Arc::new(Notify::new()),
        }
    }
}

impl FuseFactory for FlexFactory {
    fn create(&self, info: FuseInfo) -> ArcFusewire {
        Arc::new(self.build(info))
    }
}
