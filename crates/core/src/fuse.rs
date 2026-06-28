//! Low-overhead protection against stalled and abusive connections.
//!
//! A [`FusePolicy`] runs once when a connection is accepted. The resulting
//! [`FuseConfig`] is then enforced by the TLS, transport and request-body state
//! machines themselves; there is no per-I/O event dispatch or timer task.

use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use crate::conn::SocketAddr;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FuseConfig {
    /// Maximum duration of a TLS handshake.
    pub tls_handshake_timeout: Option<Duration>,
    /// Maximum duration for reading an HTTP/1 request head.
    pub http1_header_timeout: Option<Duration>,
    /// Maximum time with no successful transport read or write.
    pub connection_idle_timeout: Option<Duration>,
    /// Maximum time a requested transport write may remain pending.
    pub write_stall_timeout: Option<Duration>,
    /// Maximum gap between request-body frames.
    pub request_body_timeout: Option<Duration>,
}

impl Default for FuseConfig {
    fn default() -> Self {
        Self {
            tls_handshake_timeout: Some(Duration::from_secs(10)),
            http1_header_timeout: Some(Duration::from_secs(30)),
            connection_idle_timeout: Some(Duration::from_secs(30)),
            write_stall_timeout: Some(Duration::from_secs(30)),
            request_body_timeout: Some(Duration::from_secs(60)),
        }
    }
}

impl FuseConfig {
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

/// Selects protection settings once per accepted connection.
pub trait FusePolicy: Send + Sync + 'static {
    /// Decides whether and how to protect a connection.
    fn decide(&self, info: &FuseInfo) -> FuseAction;
}

impl FusePolicy for FuseConfig {
    fn decide(&self, _info: &FuseInfo) -> FuseAction {
        FuseAction::Accept(*self)
    }
}

impl<F> FusePolicy for F
where
    F: Fn(&FuseInfo) -> FuseAction + Send + Sync + 'static,
{
    fn decide(&self, info: &FuseInfo) -> FuseAction {
        self(info)
    }
}

/// Shared connection policy used by listeners.
pub type ArcFusePolicy = Arc<dyn FusePolicy>;
