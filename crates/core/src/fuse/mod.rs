//! This module is for protecting the server from slow or malicious clients.

mod simple;
pub use simple::{SimpleBuilder, SimpleFusewire};

use std::sync::Arc;

use async_trait::async_trait;

/// A fuse event.
#[derive(Debug, Clone, Copy)]
pub enum FuseEvent {
    /// Tls handshaking.
    TlsHandshaking,
    /// Tls handshaked.
    TlsHandshaked,
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
    fn create(&self) -> ArcFusewire;
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
pub fn simple() -> SimpleFusewire {
    SimpleFusewire::default()
}

impl<T, F> FuseFactory for T
where
    T: Fn() -> F,
    F: Fusewire + Sync + Send + 'static,
{
    fn create(&self) -> ArcFusewire {
        Arc::new((*self)())
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