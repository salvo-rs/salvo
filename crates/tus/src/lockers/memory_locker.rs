use std::collections::HashMap;
use std::sync::Arc;

use salvo_core::async_trait;
use tokio::sync::{Mutex, RwLock};

use crate::error::TusResult;
use crate::lockers::{LockGuard, Locker};

#[derive(Clone)]
pub struct MemoryLocker {
    inner: Arc<Mutex<HashMap<String, Arc<RwLock<()>>>>>,
}

impl MemoryLocker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn get_lock(&self, id: &str) -> Arc<RwLock<()>> {
        let mut map = self.inner.lock().await;
        map.entry(id.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(())))
            .clone()
    }
}

#[async_trait]
impl Locker for MemoryLocker {
    async fn lock(&self, id: &str) -> TusResult<LockGuard> {
        self.write_lock(id).await
    }

    async fn read_lock(&self, id: &str) -> TusResult<LockGuard> {
        let lock = self.get_lock(id).await;
        let guard = lock.read_owned().await;
        Ok(LockGuard::read(guard))
    }

    async fn write_lock(&self, id: &str) -> TusResult<LockGuard> {
        let lock = self.get_lock(id).await;
        let guard = lock.write_owned().await;
        Ok(LockGuard::write(guard))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn test_memory_locker_new() {
        let locker = MemoryLocker::new();
        // Should create successfully with empty inner map
        assert!(Arc::strong_count(&locker.inner) >= 1);
    }

    #[test]
    fn test_memory_locker_clone() {
        let locker1 = MemoryLocker::new();
        let locker2 = locker1.clone();
        // Both should share the same inner Arc
        assert!(Arc::ptr_eq(&locker1.inner, &locker2.inner));
    }

    #[tokio::test]
    async fn test_memory_locker_get_lock_creates_new() {
        let locker = MemoryLocker::new();

        let lock1 = locker.get_lock("upload-1").await;
        let lock2 = locker.get_lock("upload-1").await;

        // Same ID should return same lock
        assert!(Arc::ptr_eq(&lock1, &lock2));
    }

    #[tokio::test]
    async fn test_memory_locker_get_lock_different_ids() {
        let locker = MemoryLocker::new();

        let lock1 = locker.get_lock("upload-1").await;
        let lock2 = locker.get_lock("upload-2").await;

        // Different IDs should have different locks
        assert!(!Arc::ptr_eq(&lock1, &lock2));
    }

    #[tokio::test]
    async fn test_memory_locker_read_lock() {
        let locker = MemoryLocker::new();

        let guard = locker.read_lock("test-id").await;
        assert!(guard.is_ok());
    }

    #[tokio::test]
    async fn test_memory_locker_write_lock() {
        let locker = MemoryLocker::new();

        let guard = locker.write_lock("test-id").await;
        assert!(guard.is_ok());
    }

    #[tokio::test]
    async fn test_memory_locker_lock_defaults_to_write() {
        let locker = MemoryLocker::new();

        let guard = locker.lock("test-id").await;
        assert!(guard.is_ok());
    }

    #[tokio::test]
    async fn test_memory_locker_multiple_read_locks() {
        let locker = MemoryLocker::new();

        // Multiple read locks should be allowed concurrently
        let guard1 = locker.read_lock("test-id").await.unwrap();
        let guard2 = locker.read_lock("test-id").await.unwrap();

        // Both guards should be valid
        drop(guard1);
        drop(guard2);
    }

    #[tokio::test]
    async fn test_memory_locker_write_lock_exclusive() {
        use tokio::time::timeout;

        let locker = Arc::new(MemoryLocker::new());
        let counter = Arc::new(AtomicUsize::new(0));

        // Acquire write lock
        let _guard = locker.write_lock("test-id").await.unwrap();

        let locker_clone = locker.clone();
        let counter_clone = counter.clone();

        // Try to acquire another write lock in a separate task
        let handle = tokio::spawn(async move {
            // This should block until the first lock is released
            let result = timeout(Duration::from_millis(50), locker_clone.write_lock("test-id")).await;
            if result.is_ok() {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            }
        });

        // Give the spawned task time to try to acquire the lock
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Counter should still be 0 because the lock is held
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        // Drop the guard to release the lock
        drop(_guard);

        // Now the spawned task should be able to complete
        let _ = handle.await;
    }

    #[tokio::test]
    async fn test_memory_locker_different_ids_independent() {
        let locker = MemoryLocker::new();

        // Locks for different IDs should be independent
        let guard1 = locker.write_lock("id-1").await.unwrap();
        let guard2 = locker.write_lock("id-2").await.unwrap();

        // Both locks acquired successfully
        drop(guard1);
        drop(guard2);
    }

    #[tokio::test]
    async fn test_memory_locker_lock_release() {
        let locker = MemoryLocker::new();

        {
            let _guard = locker.write_lock("test-id").await.unwrap();
            // Lock is held here
        }
        // Lock is released when guard goes out of scope

        // Should be able to acquire lock again
        let guard = locker.write_lock("test-id").await;
        assert!(guard.is_ok());
    }

    #[tokio::test]
    async fn test_memory_locker_concurrent_reads_block_write() {
        use tokio::time::timeout;

        let locker = Arc::new(MemoryLocker::new());

        // Acquire read lock
        let _read_guard = locker.read_lock("test-id").await.unwrap();

        let locker_clone = locker.clone();

        // Try to acquire write lock - should block
        let result = timeout(
            Duration::from_millis(50),
            locker_clone.write_lock("test-id"),
        )
        .await;

        // Should timeout because read lock is held
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_memory_locker_many_ids() {
        let locker = MemoryLocker::new();

        // Create locks for many different IDs
        let mut guards = Vec::new();
        for i in 0..100 {
            let guard = locker.write_lock(&format!("id-{}", i)).await.unwrap();
            guards.push(guard);
        }

        // All locks should be acquired
        assert_eq!(guards.len(), 100);

        // Verify internal map has 100 entries
        let map = locker.inner.lock().await;
        assert_eq!(map.len(), 100);
    }

    #[tokio::test]
    async fn test_memory_locker_locks_persist() {
        let locker = MemoryLocker::new();

        // Create and release a lock
        {
            let _guard = locker.write_lock("test-id").await.unwrap();
        }

        // The lock entry should still exist in the map
        let map = locker.inner.lock().await;
        assert!(map.contains_key("test-id"));
    }

    #[tokio::test]
    async fn test_memory_locker_shared_state_across_clones() {
        let locker1 = MemoryLocker::new();
        let locker2 = locker1.clone();

        // Acquire lock via locker1
        let _guard = locker1.write_lock("shared-id").await.unwrap();

        // locker2 should see the same lock
        let lock1 = locker1.get_lock("shared-id").await;
        let lock2 = locker2.get_lock("shared-id").await;
        assert!(Arc::ptr_eq(&lock1, &lock2));
    }

    #[tokio::test]
    async fn test_memory_locker_empty_id() {
        let locker = MemoryLocker::new();

        // Empty string ID should work
        let guard = locker.write_lock("").await;
        assert!(guard.is_ok());
    }

    #[tokio::test]
    async fn test_memory_locker_special_characters_in_id() {
        let locker = MemoryLocker::new();

        // IDs with special characters should work
        let guard1 = locker.write_lock("id/with/slashes").await;
        assert!(guard1.is_ok());

        let guard2 = locker.write_lock("id with spaces").await;
        assert!(guard2.is_ok());

        let guard3 = locker.write_lock("id-with-特殊字符").await;
        assert!(guard3.is_ok());
    }
}
