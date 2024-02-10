//! A simple fusewire.

use std::sync::Arc;

use tokio::sync::Notify;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

use super::{async_trait, ArcFusewire, FuseEvent, FuseFactory, Fusewire, TransProto};

/// A simple fusewire.
#[derive(Default)]
pub struct SimpleFusewire {
    trans_proto: TransProto,

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

impl SimpleFusewire {
    /// Create a new `SimpleFusewire`.
    pub fn new(trans_proto: TransProto) -> Self {
        Self::builder().build(trans_proto)
    }

    /// Create a new `SimpleFactory`.
    pub fn builder() -> SimpleFactory {
        SimpleFactory::new()
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
impl Fusewire for SimpleFusewire {
    fn event(&self, event: FuseEvent) {
        if self.trans_proto.is_quic() {
            return;
        }
        self.tcp_idle_notify.notify_waiters();
        match event {
            FuseEvent::TlsHandshaking => {
                let tls_handshake_notify = self.tls_handshake_notify.clone();
                let tls_handshake_timeout = self.tls_handshake_timeout;
                let tls_handshake_token = self.tls_handshake_token.clone();
                tokio::spawn(async move {
                    loop {
                        if tokio::time::timeout(tls_handshake_timeout, tls_handshake_notify.notified())
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
            _ = self.tcp_idle_token.cancelled() => {}
            _ = self.tcp_frame_token.cancelled() => {}
            _ = self.tls_handshake_token.cancelled() => {}
        }
    }
}

/// A [`SimpleFusewire`] builder.
#[derive(Clone, Debug)]
pub struct SimpleFactory {
    tcp_idle_timeout: Duration,
    tcp_frame_timeout: Duration,
    tls_handshake_timeout: Duration,
}

impl Default for SimpleFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleFactory {
    /// Create a new `SimpleFactory`.
    pub fn new() -> Self {
        Self {
            tcp_idle_timeout: Duration::from_secs(30),
            tcp_frame_timeout: Duration::from_secs(60),
            tls_handshake_timeout: Duration::from_secs(10),
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

    /// Build a `SimpleFusewire`.
    pub fn build(&self, trans_proto: TransProto) -> SimpleFusewire {
        let Self {
            tcp_idle_timeout,
            tcp_frame_timeout,
            tls_handshake_timeout,
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
        SimpleFusewire {
            trans_proto,

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

impl FuseFactory for SimpleFactory {
    fn create(&self, trans_proto: TransProto) -> ArcFusewire {
        Arc::new(self.build(trans_proto))
    }
}
