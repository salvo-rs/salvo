//! Memory store module.
use std::borrow::Borrow;
use std::convert::Infallible;
use std::fmt::{self, Debug, Formatter};
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache as MokaCache;
use moka::future::CacheBuilder as MokaCacheBuilder;
use moka::notification::RemovalCause;

use super::{CacheStore, CachedEntry};

/// A builder for [`MokaStore`].
pub struct Builder<K> {
    inner: MokaCacheBuilder<K, CachedEntry, MokaCache<K, CachedEntry>>,
}

impl<K> Debug for Builder<K> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Builder").finish()
    }
}
impl<K> Builder<K>
where
    K: Hash + Eq + Send + Sync + Clone + 'static,
{
    /// Sets the initial capacity (number of entries) of the cache.
    #[must_use] pub fn initial_capacity(mut self, capacity: usize) -> Self {
        self.inner = self.inner.initial_capacity(capacity);
        self
    }

    /// Sets the max capacity of the cache.
    #[must_use] pub fn max_capacity(mut self, capacity: u64) -> Self {
        self.inner = self.inner.max_capacity(capacity);
        self
    }

    /// Sets the time to idle of the cache.
    ///
    /// A cached entry will expire after the specified duration has passed since `get`
    /// or `insert`.
    ///
    /// # Panics
    ///
    /// `CacheBuilder::build*` methods will panic if the given `duration` is longer
    /// than 1000 years. This is done to protect against overflow when computing key
    /// expiration.
    #[must_use] pub fn time_to_idle(mut self, duration: Duration) -> Self {
        self.inner = self.inner.time_to_idle(duration);
        self
    }

    /// Sets the time to live of the cache.
    ///
    /// A cached entry will expire after the specified duration has passed since
    /// `insert`.
    ///
    /// # Panics
    ///
    /// `CacheBuilder::build*` methods will panic if the given `duration` is longer
    /// than 1000 years. This is done to protect against overflow when computing key
    /// expiration.
    #[must_use] pub fn time_to_live(mut self, duration: Duration) -> Self {
        self.inner = self.inner.time_to_live(duration);
        self
    }

    /// Sets the eviction listener closure to the cache.
    ///
    /// # Panics
    ///
    /// It is very important to ensure the listener closure does not panic. Otherwise,
    /// the cache will stop calling the listener after a panic. This is intended
    /// behavior because the cache cannot know whether it is memory safe to
    /// call the panicked listener again.
    #[must_use]
    pub fn eviction_listener(
        mut self,
        listener: impl Fn(Arc<K>, CachedEntry, RemovalCause) + Send + Sync + 'static,
    ) -> Self {
        self.inner = self.inner.eviction_listener(listener);
        self
    }

    /// Build a [`MokaStore`].
    ///
    /// # Panics
    ///
    /// Panics if configured with either `time_to_live` or `time_to_idle` higher than
    /// 1000 years. This is done to protect against overflow when computing key
    /// expiration.
    #[must_use] pub fn build(self) -> MokaStore<K> {
        MokaStore {
            inner: self.inner.build(),
        }
    }
}
/// A simple in-memory store for rate limiter.
pub struct MokaStore<K> {
    inner: MokaCache<K, CachedEntry>,
}

impl<K> Debug for MokaStore<K> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("MokaStore").finish()
    }
}
impl<K> MokaStore<K>
where
    K: Hash + Eq + Send + Sync + Clone + 'static,
{
    /// Create a new `MokaStore`.
    #[must_use] pub fn new(max_capacity: u64) -> Self {
        Self {
            inner: MokaCache::new(max_capacity),
        }
    }

    /// Returns a [`Builder`], which can build a `MokaStore`.
    #[must_use] pub fn builder() -> Builder<K> {
        Builder {
            inner: MokaCache::builder(),
        }
    }
}

impl<K> CacheStore for MokaStore<K>
where
    K: Hash + Eq + Send + Sync + Clone + 'static,
{
    type Error = Infallible;
    type Key = K;

    async fn load_entry<Q>(&self, key: &Q) -> Option<CachedEntry>
    where
        Self::Key: Borrow<Q>,
        Q: Hash + Eq + Sync,
    {
        self.inner.get(key).await
    }

    async fn save_entry(&self, key: Self::Key, entry: CachedEntry) -> Result<(), Self::Error> {
        self.inner.insert(key, entry).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use salvo_core::http::{HeaderMap, StatusCode};
    use crate::{CachedBody, CachedEntry};

    #[tokio::test]
    async fn test_moka_store() {
        let store = MokaStore::new(100);
        let key = "test_key".to_string();
        let entry = CachedEntry {
            status: Some(StatusCode::OK),
            headers: HeaderMap::new(),
            body: CachedBody::Once("test_body".into()),
        };
        store.save_entry(key.clone(), entry.clone()).await.unwrap();
        let loaded_entry = store.load_entry(&key).await.unwrap();
        assert_eq!(loaded_entry.status, entry.status);
        assert_eq!(loaded_entry.body, entry.body);
    }

    #[tokio::test]
    async fn test_moka_store_builder() {
        let store = MokaStore::<String>::builder()
            .initial_capacity(50)
            .max_capacity(100)
            .time_to_live(Duration::from_secs(1))
            .time_to_idle(Duration::from_secs(1))
            .build();
        let key = "test_key".to_string();
        let entry = CachedEntry {
            status: Some(StatusCode::OK),
            headers: HeaderMap::new(),
            body: CachedBody::Once("test_body".into()),
        };
        store.save_entry(key.clone(), entry.clone()).await.unwrap();
        let loaded_entry = store.load_entry(&key).await.unwrap();
        assert_eq!(loaded_entry.status, entry.status);
        assert_eq!(loaded_entry.body, entry.body);

        tokio::time::sleep(Duration::from_secs(2)).await;
        let loaded_entry = store.load_entry(&key).await;
        assert!(loaded_entry.is_none());
    }
    
    #[test]
    fn test_builder_debug() {
        let builder = MokaStore::<String>::builder();
        let dbg_str = format!("{:?}", builder);
        assert_eq!(dbg_str, "Builder");
    }

    #[test]
    fn test_moka_store_debug() {
        let store = MokaStore::<String>::new(100);
        let dbg_str = format!("{:?}", store);
        assert_eq!(dbg_str, "MokaStore");
    }
    
    #[tokio::test]
    async fn test_eviction_listener() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let evicted = Arc::new(AtomicBool::new(false));
        let evicted_clone = evicted.clone();
        let store = MokaStore::<String>::builder()
            .max_capacity(1)
            .eviction_listener(move |_, _, _| {
                evicted_clone.store(true, Ordering::SeqCst);
            })
            .build();
        let entry = CachedEntry {
            status: None,
            headers: HeaderMap::new(),
            body: CachedBody::Once("test_body".into()),
        };
        store.save_entry("key1".to_string(), entry.clone()).await.unwrap();
        store.save_entry("key2".to_string(), entry.clone()).await.unwrap();
        
        // Try to get the key to give time to the eviction listener to run.
        for _ in 0..10 {
            store.load_entry(&"key1".to_string()).await;
            store.load_entry(&"key2".to_string()).await;
            if evicted.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        assert!(evicted.load(Ordering::SeqCst));
    }
}