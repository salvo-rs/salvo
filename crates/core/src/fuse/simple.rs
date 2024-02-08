use std::sync::Arc;

use tokio::sync::Notify;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

use super::{async_trait, FuseEvent, Fusewire, TransProto};

/// A simple fusewire.
#[derive(Default)]
pub struct SimpleFusewire {
    trans_proto: TransProto,
    tcp_idle_token: CancellationToken,
    tcp_idle_notify: Arc<Notify>,

    tcp_idle_timeout: Duration,
    tcp_frame_timeout: Duration,
    tls_handshake_timeout: Duration,
}

impl SimpleFusewire {
    /// Create a new `SimpleFusewire`.
    pub fn new(trans_proto: TransProto) -> Self {
        Self::builder().build(trans_proto)
    }

    /// Create a new `SimpleBuilder`.
    pub fn builder() -> SimpleBuilder {
        SimpleBuilder::new()
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

/// A [`SimpleFusewire`] builder.
pub struct SimpleBuilder {
    tcp_idle_timeout: Duration,
    tcp_frame_timeout: Duration,
    tls_handshake_timeout: Duration,
}
impl SimpleBuilder {
    /// Create a new `SimpleBuilder`.
    pub fn new() -> Self {
        Self {
            tcp_idle_timeout: Duration::from_secs(10),
            tcp_frame_timeout: Duration::from_secs(10),
            tls_handshake_timeout: Duration::from_secs(5),
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
    pub fn build(self, trans_proto: TransProto) -> SimpleFusewire {
        let Self {
            tcp_idle_timeout,
            tcp_frame_timeout,
            tls_handshake_timeout,
        } = self;

        let tcp_idle_token = CancellationToken::new();
        let tcp_idle_notify = Arc::new(Notify::new());
        SimpleFusewire {
            trans_proto,
            tcp_idle_token,
            tcp_idle_notify,
            tcp_idle_timeout,
            tcp_frame_timeout,
            tls_handshake_timeout,
        }
    }
}

#[async_trait]
impl Fusewire for SimpleFusewire {
    fn event(&self, event: FuseEvent) {
        // self.tcp_idle_notify.notify();
    }
    async fn fused(&self) {
        futures_util::future::pending::<()>().await;
    }
}
