//! This module is for protecting the server from slow or malicious clients.

mod simple;
pub use simple::{SimpleBuilder, SimpleFusewire};

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
    /// RecvFrame.
    RecvFrame,
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
    /// Report a event.
    fn event(&self, event: FuseEvent);
    /// Check if the fuse is fused.
    async fn fused(&self);
}

/// Create a pseudo fusewire.
pub fn pseudo() -> PseudoFusewire {
    PseudoFusewire
}
/// Create a simple fusewire.
pub fn simple(trans_proto: TransProto) -> SimpleFusewire {
    SimpleFusewire::new(trans_proto)
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

impl FuseFactory for PseudoFusewire {
    fn create(&self, _trans_proto: TransProto) -> ArcFusewire {
        Arc::new(PseudoFusewire)
    }
}

/// A pseudo fusewire.
pub struct PseudoFusewire;
#[async_trait]
impl Fusewire for PseudoFusewire {
    fn event(&self, _event: FuseEvent) {}
    async fn fused(&self) {
        futures_util::future::pending::<()>().await;
    }
}
