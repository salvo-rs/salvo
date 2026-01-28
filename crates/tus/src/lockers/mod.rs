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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn test_lock_guard_read() {
        let lock = Arc::new(RwLock::new(()));
        let guard = lock.read_owned().await;
        let lock_guard = LockGuard::read(guard);

        // Guard should exist and hold the lock
        drop(lock_guard);
        // Lock should be released after drop
    }

    #[tokio::test]
    async fn test_lock_guard_write() {
        let lock = Arc::new(RwLock::new(()));
        let guard = lock.write_owned().await;
        let lock_guard = LockGuard::write(guard);

        // Guard should exist and hold the lock
        drop(lock_guard);
        // Lock should be released after drop
    }

    #[tokio::test]
    async fn test_lock_guard_read_allows_multiple() {
        let lock = Arc::new(RwLock::new(()));

        let guard1 = lock.clone().read_owned().await;
        let lock_guard1 = LockGuard::read(guard1);

        let guard2 = lock.clone().read_owned().await;
        let lock_guard2 = LockGuard::read(guard2);

        // Both read guards should coexist
        drop(lock_guard1);
        drop(lock_guard2);
    }

    #[tokio::test]
    async fn test_lock_guard_write_is_exclusive() {
        use tokio::time::{timeout, Duration};

        let lock = Arc::new(RwLock::new(()));
        let lock_clone = lock.clone();

        let guard = lock.write_owned().await;
        let _lock_guard = LockGuard::write(guard);

        // Try to acquire another write lock - should fail with timeout
        let result = timeout(Duration::from_millis(10), lock_clone.write_owned()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lock_guard_drop_releases_read() {
        let lock = Arc::new(RwLock::new(()));

        {
            let guard = lock.clone().read_owned().await;
            let _lock_guard = LockGuard::read(guard);
            // Lock is held here
        }
        // Lock should be released

        // Should be able to acquire write lock now
        let write_guard = lock.try_write_owned();
        assert!(write_guard.is_ok());
    }

    #[tokio::test]
    async fn test_lock_guard_drop_releases_write() {
        let lock = Arc::new(RwLock::new(()));

        {
            let guard = lock.clone().write_owned().await;
            let _lock_guard = LockGuard::write(guard);
            // Lock is held here
        }
        // Lock should be released

        // Should be able to acquire another write lock now
        let write_guard = lock.try_write_owned();
        assert!(write_guard.is_ok());
    }

    #[tokio::test]
    async fn test_locker_trait_default_methods() {
        use memory_locker::MemoryLocker;

        let locker = MemoryLocker::new();

        // Test that read_lock defaults to lock behavior
        let guard1 = locker.read_lock("test").await;
        assert!(guard1.is_ok());
        drop(guard1);

        // Test that write_lock defaults to lock behavior
        let guard2 = locker.write_lock("test").await;
        assert!(guard2.is_ok());
        drop(guard2);

        // Test lock itself
        let guard3 = locker.lock("test").await;
        assert!(guard3.is_ok());
    }
}
