//! Protecting the server from slow HTTP attacks.

pub mod simple;
pub use simple::{SimpleFactory, SimpleFusewire};

use std::sync::Arc;

use async_trait::async_trait;

/// A transport protocol.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransProto {
    /// Tcp.
    #[default]
    Tcp,
    /// Quic.
    Quic,
}
impl TransProto {
    /// Check if the transport protocol is tcp.
    pub fn is_tcp(&self) -> bool {
        matches!(self, Self::Tcp)
    }
    /// Check if the transport protocol is quic.
    pub fn is_quic(&self) -> bool {
        matches!(self, Self::Quic)
    }
}

/// A fuse event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FuseEvent {
    /// Tls handshaking.
    TlsHandshaking,
    /// Tls handshaked.
    TlsHandshaked,
    /// Alive.
    Alive,
    /// ReadData.
    ReadData(usize),
    /// WriteData.
    WriteData(usize),
    /// WaitFrame.
    WaitFrame,
    /// GainFrame.
    GainFrame,
}

pub(crate) type ArcFuseFactory = Arc<dyn FuseFactory + Sync + Send + 'static>;
pub(crate) type ArcFusewire = Arc<dyn Fusewire + Sync + Send + 'static>;

/// A fuse factory.
pub trait FuseFactory {
    /// Create a new fusewire.
    fn create(&self, trans_proto: TransProto) -> ArcFusewire;
}

/// A fusewire.
#[async_trait]
pub trait Fusewire {
    /// Recive a event report.
    fn event(&self, event: FuseEvent);
    /// Check if the fusewire is fused.
    async fn fused(&self);
}

impl<T, F> FuseFactory for T
where
    T: Fn(TransProto) -> F,
    F: Fusewire + Sync + Send + 'static,
{
    fn create(&self, trans_proto: TransProto) -> ArcFusewire {
        Arc::new((*self)(trans_proto))
    }
}

impl FuseFactory for SteadyFusewire {
    fn create(&self, _trans_proto: TransProto) -> ArcFusewire {
        Arc::new(SteadyFusewire)
    }
}

/// A fusewire never fused.
///
/// This fusewire will do nothing.
pub struct SteadyFusewire;
#[async_trait]
impl Fusewire for SteadyFusewire {
    fn event(&self, _event: FuseEvent) {}
    async fn fused(&self) {
        futures_util::future::pending::<()>().await;
    }
}
