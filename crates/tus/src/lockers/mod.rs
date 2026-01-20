use salvo_core::async_trait;
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard};

pub mod memory_locker;

use crate::error::TusResult;

#[async_trait]
pub trait Locker: Send + Sync + 'static {
    async fn lock(&self, id: &str) -> TusResult<LockGuard>;
    async fn read_lock(&self, id: &str) -> TusResult<LockGuard> {
        self.lock(id).await
    }
    async fn write_lock(&self, id: &str) -> TusResult<LockGuard> {
        self.lock(id).await
    }
}

pub struct LockGuard {
    _guard: LockGuardInner,
}

#[allow(dead_code)]
enum LockGuardInner {
    Read(OwnedRwLockReadGuard<()>),
    Write(OwnedRwLockWriteGuard<()>),
}

impl LockGuard {
    pub(crate) fn read(guard: OwnedRwLockReadGuard<()>) -> Self {
        Self {
            _guard: LockGuardInner::Read(guard),
        }
    }

    pub(crate) fn write(guard: OwnedRwLockWriteGuard<()>) -> Self {
        Self {
            _guard: LockGuardInner::Write(guard),
        }
    }
}
