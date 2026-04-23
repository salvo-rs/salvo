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
use std::sync::{Arc, Mutex, MutexGuard};

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

#[derive(Debug, Default)]
struct TimeoutWatchState {
    armed: bool,
    generation: u64,
    // A cancellation token records disarm even if the watcher starts polling later.
    cancel_token: Option<CancellationToken>,
}

#[derive(Debug)]
struct TimeoutWatch {
    generation: u64,
    cancel_token: CancellationToken,
}

type TimeoutWatchStateRef = Arc<Mutex<TimeoutWatchState>>;

/// A flexible, configurable fusewire implementation.
///
/// `FlexFusewire` monitors a single connection and terminates it if any of
/// the configured timeouts are exceeded or if a guard rejects the connection.
///
/// # Timeout Behavior
///
/// - **TCP Idle Timeout**: Connection is terminated if no activity occurs within the idle timeout
///   period. Any event resets this timer.
///
/// - **TCP Frame Timeout**: After a `WaitFrame` event, the connection is terminated if a complete
///   frame is not received within the frame timeout period.
///
/// - **TLS Handshake Timeout**: During TLS negotiation, the connection is terminated if the
///   handshake does not complete within the handshake timeout period.
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
    tcp_frame_timeout_state: TimeoutWatchStateRef,

    tls_handshake_timeout: Duration,
    tls_handshake_token: CancellationToken,
    tls_handshake_timeout_state: TimeoutWatchStateRef,
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

    fn arm_timeout(state: &TimeoutWatchStateRef) -> Option<TimeoutWatch> {
        let mut state = Self::lock_timeout_state(state);
        if state.armed {
            None
        } else {
            state.armed = true;
            Self::advance_generation(&mut state);
            let cancel_token = CancellationToken::new();
            state.cancel_token = Some(cancel_token.clone());
            Some(TimeoutWatch {
                generation: state.generation,
                cancel_token,
            })
        }
    }

    fn disarm_timeout(state: &TimeoutWatchStateRef) -> bool {
        let (was_armed, cancel_token) = {
            let mut state = Self::lock_timeout_state(state);
            if state.armed {
                state.armed = false;
                Self::advance_generation(&mut state);
                (true, state.cancel_token.take())
            } else {
                (false, None)
            }
        };
        if let Some(cancel_token) = cancel_token {
            cancel_token.cancel();
        }
        was_armed
    }

    fn finish_timeout(state: &TimeoutWatchStateRef, generation: u64) -> bool {
        let mut state = Self::lock_timeout_state(state);
        if state.armed && state.generation == generation {
            state.armed = false;
            Self::advance_generation(&mut state);
            state.cancel_token.take();
            true
        } else {
            false
        }
    }

    fn arm_timeout_task(
        timeout: Duration,
        fuse_token: CancellationToken,
        timeout_state: TimeoutWatchStateRef,
    ) {
        let Some(watch) = Self::arm_timeout(&timeout_state) else {
            return;
        };
        Self::spawn_timeout_task(timeout, fuse_token, timeout_state, watch);
    }

    fn spawn_timeout_task(
        timeout: Duration,
        fuse_token: CancellationToken,
        timeout_state: TimeoutWatchStateRef,
        watch: TimeoutWatch,
    ) {
        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(timeout) => {
                    if Self::finish_timeout(&timeout_state, watch.generation) {
                        fuse_token.cancel();
                    }
                }
                _ = watch.cancel_token.cancelled() => {}
            }
        });
    }

    #[cfg(test)]
    fn timeout_state_is_armed(state: &TimeoutWatchStateRef) -> bool {
        Self::lock_timeout_state(state).armed
    }

    fn lock_timeout_state<'a>(
        state: &'a TimeoutWatchStateRef,
    ) -> MutexGuard<'a, TimeoutWatchState> {
        state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn advance_generation(state: &mut TimeoutWatchState) {
        state.generation = state.generation.wrapping_add(1);
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
                Self::arm_timeout_task(
                    self.tls_handshake_timeout,
                    self.tls_handshake_token.clone(),
                    self.tls_handshake_timeout_state.clone(),
                );
            }
            FuseEvent::TlsHandshaked => {
                Self::disarm_timeout(&self.tls_handshake_timeout_state);
            }
            FuseEvent::WaitFrame => {
                Self::arm_timeout_task(
                    self.tcp_frame_timeout,
                    self.tcp_frame_token.clone(),
                    self.tcp_frame_timeout_state.clone(),
                );
            }
            FuseEvent::GainFrame => {
                Self::disarm_timeout(&self.tcp_frame_timeout_state);
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
            tcp_frame_timeout_state: Arc::new(Mutex::new(TimeoutWatchState::default())),

            tls_handshake_timeout,
            tls_handshake_token: CancellationToken::new(),
            tls_handshake_timeout_state: Arc::new(Mutex::new(TimeoutWatchState::default())),
        }
    }
}

impl FuseFactory for FlexFactory {
    fn create(&self, info: FuseInfo) -> ArcFusewire {
        Arc::new(self.build(info))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::fuse::TransProto;

    fn test_info() -> FuseInfo {
        FuseInfo {
            trans_proto: TransProto::Tcp,
            remote_addr: std::net::SocketAddr::from(([127, 0, 0, 1], 4000)).into(),
            local_addr: std::net::SocketAddr::from(([127, 0, 0, 1], 8080)).into(),
        }
    }

    async fn wait_for_timeout_state_owners(state: &TimeoutWatchStateRef, expected_count: usize) {
        tokio::time::timeout(Duration::from_millis(100), async {
            while Arc::strong_count(state) != expected_count {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn wait_frame_reuses_a_single_timeout_task() {
        let fusewire = FlexFactory::new()
            .tcp_frame_timeout(Duration::from_secs(1))
            .build(test_info());

        for _ in 0..8 {
            fusewire.event(FuseEvent::WaitFrame);
        }
        tokio::task::yield_now().await;

        assert_eq!(Arc::strong_count(&fusewire.tcp_frame_timeout_state), 2);
        assert!(FlexFusewire::timeout_state_is_armed(
            &fusewire.tcp_frame_timeout_state
        ));

        fusewire.event(FuseEvent::GainFrame);
        wait_for_timeout_state_owners(&fusewire.tcp_frame_timeout_state, 1).await;

        assert!(!FlexFusewire::timeout_state_is_armed(
            &fusewire.tcp_frame_timeout_state
        ));
    }

    #[tokio::test]
    async fn wait_frame_can_rearm_without_losing_the_new_timeout() {
        let fusewire = FlexFactory::new()
            .tcp_frame_timeout(Duration::from_millis(20))
            .build(test_info());

        fusewire.event(FuseEvent::WaitFrame);
        tokio::task::yield_now().await;

        fusewire.event(FuseEvent::GainFrame);
        fusewire.event(FuseEvent::WaitFrame);
        tokio::task::yield_now().await;

        assert!(FlexFusewire::timeout_state_is_armed(
            &fusewire.tcp_frame_timeout_state
        ));

        tokio::select! {
            _ = fusewire.fused() => {}
            _ = tokio::time::sleep(Duration::from_millis(60)) => {
                panic!("re-armed frame timeout should still be able to fuse the connection")
            }
        }
    }

    #[tokio::test]
    async fn disarm_before_timeout_task_polls_is_observed() {
        let fusewire = FlexFactory::new()
            .tcp_frame_timeout(Duration::from_secs(60))
            .build(test_info());

        let watch = FlexFusewire::arm_timeout(&fusewire.tcp_frame_timeout_state)
            .expect("first generation should arm");
        assert!(FlexFusewire::disarm_timeout(
            &fusewire.tcp_frame_timeout_state
        ));
        FlexFusewire::spawn_timeout_task(
            fusewire.tcp_frame_timeout,
            fusewire.tcp_frame_token.clone(),
            fusewire.tcp_frame_timeout_state.clone(),
            watch,
        );
        wait_for_timeout_state_owners(&fusewire.tcp_frame_timeout_state, 1).await;
        assert!(!FlexFusewire::timeout_state_is_armed(
            &fusewire.tcp_frame_timeout_state
        ));

        tokio::select! {
            _ = fusewire.fused() => panic!("disarmed timeout watcher must not fuse the connection"),
            _ = tokio::time::sleep(Duration::from_millis(20)) => {}
        }
    }

    #[tokio::test]
    async fn tls_handshake_completion_stops_its_timeout_task() {
        let fusewire = FlexFactory {
            tls_handshake_timeout: Duration::from_millis(20),
            ..FlexFactory::new()
        }
        .build(test_info());

        fusewire.event(FuseEvent::TlsHandshaking);
        tokio::task::yield_now().await;

        assert_eq!(Arc::strong_count(&fusewire.tls_handshake_timeout_state), 2);
        assert!(FlexFusewire::timeout_state_is_armed(
            &fusewire.tls_handshake_timeout_state
        ));

        fusewire.event(FuseEvent::TlsHandshaked);
        wait_for_timeout_state_owners(&fusewire.tls_handshake_timeout_state, 1).await;

        tokio::select! {
            _ = fusewire.fused() => panic!("completed handshakes must not trip the timeout"),
            _ = tokio::time::sleep(Duration::from_millis(40)) => {}
        }

        assert!(!FlexFusewire::timeout_state_is_armed(
            &fusewire.tls_handshake_timeout_state
        ));
    }
}
