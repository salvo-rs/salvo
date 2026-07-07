//! Low-overhead protection against stalled and abusive connections.
//!
//! A [`FusePolicy`] runs once when a connection is accepted. The resulting
//! [`FuseConfig`] is then enforced by the TLS, transport and request-body state
//! machines themselves; there is no per-I/O event dispatch or timer task.

use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use crate::async_trait;
use crate::conn::{ConnCtrl, SocketAddr};

/// Transport used by an accepted connection.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransProto {
    /// TCP or a TCP-based protocol.
    #[default]
    Tcp,
    /// QUIC.
    Quic,
}

/// Metadata available to a connection admission policy.
#[derive(Clone, Debug)]
pub struct FuseInfo {
    /// Transport protocol.
    pub trans_proto: TransProto,
    /// Peer address.
    pub remote_addr: SocketAddr,
    /// Local listener address.
    pub local_addr: SocketAddr,
}

/// Timeouts applied to an accepted connection.
///
/// Three presets mark the useful points on the spectrum, and the `with_*` builders adjust any
/// field from there:
///
/// - [`default`](Self::default) — the **safe defaults** a server applies out of the box: only the
///   TLS-handshake and HTTP/1 header timeouts, which stop slow-handshake / Slowloris attacks and
///   never trip on a legitimate client.
/// - [`strict`](Self::strict) — **every** timeout, adding idle, write-stall and request-body
///   limits. The strongest protection, but it can close valid long-lived or slow connections.
/// - [`disabled`](Self::disabled) — no timeouts at all.
///
/// ```
/// use std::time::Duration;
///
/// use salvo_core::fuse::FuseConfig;
///
/// let config = FuseConfig::default().with_connection_idle_timeout(Duration::from_secs(60));
/// ```
///
/// It is `#[non_exhaustive]`, so new timeout knobs can be added without breaking callers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FuseConfig {
    /// Maximum duration of a TLS handshake.
    pub tls_handshake_timeout: Option<Duration>,
    /// Maximum duration for reading an HTTP/1 request head.
    ///
    /// This is the single source of truth for the HTTP/1 header-read timeout. When set it
    /// bounds *both* the initial protocol-detection read and Hyper's own header read (sharing
    /// one deadline across the two), and it takes precedence over any value configured through
    /// [`Server::http1_mut`](crate::Server::http1_mut). Leave it `None` (or
    /// [`disable_fuse`](crate::Server::disable_fuse)) to leave the detection read unbounded and
    /// keep whatever the Hyper builder was given for its own header read.
    pub http1_header_timeout: Option<Duration>,
    /// Maximum time with no successful transport read or write.
    ///
    /// Enforced for TCP-based transports (plain TCP, TLS, Unix sockets). It does **not** apply
    /// to HTTP/3: a QUIC connection multiplexes independent streams and has no single byte
    /// stream to time, so idleness there is governed by QUIC's own transport-level
    /// `max_idle_timeout`, configured on the QUIC listener rather than through this field.
    pub connection_idle_timeout: Option<Duration>,
    /// Maximum time a requested transport write may remain pending.
    ///
    /// Enforced for TCP-based transports only. It does **not** apply to HTTP/3, where each QUIC
    /// stream has independent flow control and there is no connection-wide pending write to
    /// bound.
    pub write_stall_timeout: Option<Duration>,
    /// Maximum gap between request-body frames.
    pub request_body_timeout: Option<Duration>,
}

impl Default for FuseConfig {
    /// The safe defaults: the TLS-handshake and HTTP/1 header timeouts only.
    ///
    /// These defend against slow-handshake and slow-header (Slowloris) attacks and cannot trip
    /// on any legitimate client — no real client takes ten seconds to handshake or thirty to
    /// send its request head. The idle, write-stall and request-body timeouts are left off
    /// because they *can* close otherwise-valid connections: idle WebSocket / SSE / long-poll
    /// sessions, or slow-but-progressing uploads and downloads. Turn them on with
    /// [`strict`](Self::strict) or the `with_*` builders once you know your workload has none
    /// of those (or rely on the WebSocket support, which relaxes them on upgrade).
    fn default() -> Self {
        Self {
            tls_handshake_timeout: Some(Duration::from_secs(10)),
            http1_header_timeout: Some(Duration::from_secs(30)),
            connection_idle_timeout: None,
            write_stall_timeout: None,
            request_body_timeout: None,
        }
    }
}

impl FuseConfig {
    /// Enables every fuse timeout, including the ones [`default`](Self::default) leaves off.
    ///
    /// On top of the handshake and header timeouts this adds the idle, write-stall and
    /// request-body limits. It is the strongest protection but can close valid long-lived or
    /// slow connections, so opt in only when your workload has none — or pair it with handlers
    /// that call [`ConnCtrl::relax_timeouts`](crate::conn::ConnCtrl::relax_timeouts) on
    /// upgrade, as the built-in WebSocket support already does.
    #[must_use]
    pub const fn strict() -> Self {
        Self {
            tls_handshake_timeout: Some(Duration::from_secs(10)),
            http1_header_timeout: Some(Duration::from_secs(30)),
            connection_idle_timeout: Some(Duration::from_secs(30)),
            write_stall_timeout: Some(Duration::from_secs(30)),
            request_body_timeout: Some(Duration::from_secs(60)),
        }
    }

    /// Disables every fuse timeout.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            tls_handshake_timeout: None,
            http1_header_timeout: None,
            connection_idle_timeout: None,
            write_stall_timeout: None,
            request_body_timeout: None,
        }
    }

    /// Sets the maximum duration of a TLS handshake.
    #[must_use]
    pub fn with_tls_handshake_timeout(mut self, timeout: impl Into<Option<Duration>>) -> Self {
        self.tls_handshake_timeout = timeout.into();
        self
    }

    /// Sets the maximum duration for reading an HTTP/1 request head.
    #[must_use]
    pub fn with_http1_header_timeout(mut self, timeout: impl Into<Option<Duration>>) -> Self {
        self.http1_header_timeout = timeout.into();
        self
    }

    /// Sets the maximum time with no successful transport read or write.
    #[must_use]
    pub fn with_connection_idle_timeout(mut self, timeout: impl Into<Option<Duration>>) -> Self {
        self.connection_idle_timeout = timeout.into();
        self
    }

    /// Sets the maximum time a requested transport write may remain pending.
    #[must_use]
    pub fn with_write_stall_timeout(mut self, timeout: impl Into<Option<Duration>>) -> Self {
        self.write_stall_timeout = timeout.into();
        self
    }

    /// Sets the maximum gap between request-body frames.
    #[must_use]
    pub fn with_request_body_timeout(mut self, timeout: impl Into<Option<Duration>>) -> Self {
        self.request_body_timeout = timeout.into();
        self
    }
}

/// Admission result for a newly accepted connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FuseAction {
    /// Accept and enforce this configuration.
    ///
    /// Use Accept(FuseConfig::disabled()) to accept a connection without any timeouts.
    Accept(FuseConfig),
    /// Drop the connection before protocol handling.
    Reject,
}

/// A per-connection observer for transport activity.
///
/// The fuse system enforces protection with fixed inline timeouts, which covers almost
/// every server. This trait is the escape hatch for the rest: a [`FusePolicy`] may attach a
/// custom observer to feed the bytes a connection transfers into adaptive rate limiting,
/// metrics, or an external security system — the kind of per-event logic the timeout knobs
/// cannot express.
///
/// An observer can also *act*, not just watch. [`FusePolicy::observe`] hands it the
/// connection's [`ConnCtrl`], so a slow-read or abuse detector can call
/// [`ConnCtrl::abort`] or [`ConnCtrl::graceful_shutdown`] once its own logic fires — the
/// detect-then-terminate loop the fixed timeouts cannot express on their own.
///
/// Attaching an observer is opt-in. When a policy returns no observer (the default), the
/// transport hot path allocates nothing and dispatches nothing.
///
/// Only transport reads and writes are reported. The TLS handshake and the request body have
/// dedicated timeouts ([`tls_handshake_timeout`](FuseConfig::tls_handshake_timeout),
/// [`request_body_timeout`](FuseConfig::request_body_timeout)) and are not surfaced here.
/// New reporting points can be added later as further defaulted methods without breaking
/// existing implementations.
pub trait ConnObserver: Send + Sync + 'static {
    /// Called after `bytes` (always non-zero) were read from the transport.
    fn on_read(&self, bytes: usize) {
        let _ = bytes;
    }
    /// Called after `bytes` (always non-zero) were written to the transport.
    fn on_write(&self, bytes: usize) {
        let _ = bytes;
    }
}

/// Shared per-connection transport observer.
pub type ArcConnObserver = Arc<dyn ConnObserver>;

/// Selects protection settings once per accepted connection.
///
/// [`decide`](Self::decide) is `async` so admission can consult external state — a
/// blocklist, a per-IP counter, a shared rate limiter — before a connection is served. It
/// runs once, on the accept path, so keep it cheap; heavy work there serializes accepts.
#[async_trait]
pub trait FusePolicy: Send + Sync + 'static {
    /// Decides whether and how to protect a connection.
    async fn decide(&self, info: &FuseInfo) -> FuseAction;

    /// Creates an optional [`ConnObserver`] for an accepted connection.
    ///
    /// Returns `None` by default, which keeps the transport hot path free of any observer
    /// dispatch. Override it to attach custom per-connection monitoring; it is called once,
    /// right after [`decide`](Self::decide) admits the connection.
    ///
    /// `ctrl` is the accepted connection's control. Clone it into the returned observer to let
    /// that observer terminate the connection ([`ConnCtrl::abort`] /
    /// [`ConnCtrl::graceful_shutdown`]) when its own detection logic decides to.
    fn observe(&self, info: &FuseInfo, ctrl: &ConnCtrl) -> Option<ArcConnObserver> {
        let _ = (info, ctrl);
        None
    }
}

#[async_trait]
impl FusePolicy for FuseConfig {
    async fn decide(&self, _info: &FuseInfo) -> FuseAction {
        FuseAction::Accept(*self)
    }
}

#[async_trait]
impl<F> FusePolicy for F
where
    F: Fn(&FuseInfo) -> FuseAction + Send + Sync + 'static,
{
    async fn decide(&self, info: &FuseInfo) -> FuseAction {
        self(info)
    }
}

/// Shared connection policy used by listeners.
pub type ArcFusePolicy = Arc<dyn FusePolicy>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_enables_only_the_harmless_timeouts() {
        let config = FuseConfig::default();
        // Slowloris-family defenses that never trip a legitimate client stay on...
        assert!(config.tls_handshake_timeout.is_some());
        assert!(config.http1_header_timeout.is_some());
        // ...while the timeouts that can close valid long-lived / slow connections stay off.
        assert!(config.connection_idle_timeout.is_none());
        assert!(config.write_stall_timeout.is_none());
        assert!(config.request_body_timeout.is_none());
    }

    #[test]
    fn strict_enables_every_timeout() {
        let config = FuseConfig::strict();
        assert!(config.tls_handshake_timeout.is_some());
        assert!(config.http1_header_timeout.is_some());
        assert!(config.connection_idle_timeout.is_some());
        assert!(config.write_stall_timeout.is_some());
        assert!(config.request_body_timeout.is_some());
    }

    #[tokio::test]
    async fn async_policy_can_await_before_admission() {
        struct Blocklist;
        #[async_trait]
        impl FusePolicy for Blocklist {
            async fn decide(&self, info: &FuseInfo) -> FuseAction {
                // Stands in for an async lookup (blocklist store, per-IP counter, ...).
                tokio::task::yield_now().await;
                if info.remote_addr.as_ipv4().is_some() {
                    FuseAction::Reject
                } else {
                    FuseAction::Accept(FuseConfig::disabled())
                }
            }
        }

        let policy: ArcFusePolicy = Arc::new(Blocklist);
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
        let info = FuseInfo {
            trans_proto: TransProto::Tcp,
            remote_addr: addr.into(),
            local_addr: addr.into(),
        };
        assert_eq!(policy.decide(&info).await, FuseAction::Reject);
    }
}
