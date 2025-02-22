//! A flexible fusewire.

use std::sync::Arc;

use tokio::sync::Notify;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

use super::{ArcFusewire, FuseEvent, FuseFactory, FuseInfo, Fusewire, async_trait};

/// A guard action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GuardAction {
    /// Reject the connection.
    Reject,
    /// Allow the event to next guards.
    ToNext,
    /// Permit the event and skip next guards.
    Permit,
}
/// A guard.
pub trait Guard: Sync + Send + 'static {
    /// Check the event.
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

/// Skip the quic connection.
pub fn skip_quic(info: &FuseInfo, _event: &FuseEvent) -> GuardAction {
    if info.trans_proto.is_quic() {
        GuardAction::Permit
    } else {
        GuardAction::ToNext
    }
}

/// A simple fusewire.
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
impl FlexFusewire {
    /// Create a new `FlexFusewire`.
    pub fn new(info: FuseInfo) -> Self {
        Self::builder().build(info)
    }

    /// Create a new `FlexFactory`.
    pub fn builder() -> FlexFactory {
        FlexFactory::new()
    }
    /// Get the timeout for close the idle tcp connection.
    pub fn tcp_idle_timeout(&self) -> Duration {
        self.tcp_idle_timeout
    }
    /// Get the timeout for close the connection if frame can not be recived.
    pub fn tcp_frame_timeout(&self) -> Duration {
        self.tcp_frame_timeout
    }
    /// Set the timeout for close the connection if handshake not finished.
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
                _ => {}
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

/// A [`FlexFusewire`] builder.
#[derive(Clone)]
pub struct FlexFactory {
    tcp_idle_timeout: Duration,
    tcp_frame_timeout: Duration,
    tls_handshake_timeout: Duration,

    guards: Arc<Vec<Box<dyn Guard>>>,
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
    pub fn tcp_idle_timeout(mut self, timeout: Duration) -> Self {
        self.tcp_idle_timeout = timeout;
        self
    }
    /// Set the timeout for close the connection if frame can not be recived.
    pub fn tcp_frame_timeout(mut self, timeout: Duration) -> Self {
        self.tcp_frame_timeout = timeout;
        self
    }

    /// Set guards to new value.
    pub fn guards(mut self, guards: Vec<Box<dyn Guard>>) -> Self {
        self.guards = Arc::new(guards);
        self
    }
    /// Add a guard.
    pub fn add_guard(mut self, guard: impl Guard) -> Self {
        Arc::get_mut(&mut self.guards)
            .expect("guards get mut failed")
            .push(Box::new(guard));
        self
    }

    /// Build a `FlexFusewire`.
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
