use salvo_core::async_trait;
use tokio::sync::OwnedMutexGuard;

pub mod memory_locker;

use crate::error::TusResult;

#[async_trait]
pub trait Locker: Send + Sync + 'static {
    async fn lock(&self, id: &str) -> TusResult<LockGuard>;
}

pub struct LockGuard {
    _guard: OwnedMutexGuard<()>,
}
