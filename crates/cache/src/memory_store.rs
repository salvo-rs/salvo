//! memory store module.
use std::borrow::Borrow;
use std::convert::Infallible;
use std::hash::Hash;
use std::time::Duration;

use moka::sync::Cache as MokaCache;
use moka::sync::CacheBuilder as MokaCacheBuilder;
use salvo_core::async_trait;

use super::{CacheStore, CachedEntry};

/// A builder for [`MemoryStore`].
pub struct Builder<K> {
    inner: MokaCacheBuilder<K, CachedEntry, MokaCache<K, CachedEntry>>,
}
impl<K> Builder<K>
where
    K: Hash + Eq + Send + Sync + Clone + 'static,
{
    /// Sets the initial capacity (number of entries) of the cache.
    pub fn initial_capacity(mut self, capacity: usize) -> Self {
        self.inner = self.inner.initial_capacity(capacity);
        self
    }

    /// Sets the max capacity of the cache.
    pub fn max_capacity(mut self, capacity: u64) -> Self {
        self.inner = self.inner.max_capacity(capacity);
        self
    }

    /// Sets the time to idle of the cache.
    ///
    /// A cached entry will be expired after the specified duration past from `get`
    /// or `insert`.
    ///
    /// # Panics
    ///
    /// `CacheBuilder::build*` methods will panic if the given `duration` is longer
    /// than 1000 years. This is done to protect against overflow when computing key
    /// expiration.
    pub fn time_to_idle(mut self, duration: Duration) -> Self {
        self.inner = self.inner.time_to_idle(duration);
        self
    }

    /// Sets the time to live of the cache.
    ///
    /// A cached entry will be expired after the specified duration past from
    /// `insert`.
    ///
    /// # Panics
    ///
    /// `CacheBuilder::build*` methods will panic if the given `duration` is longer
    /// than 1000 years. This is done to protect against overflow when computing key
    /// expiration.
    pub fn time_to_live(mut self, duration: Duration) -> Self {
        self.inner = self.inner.time_to_live(duration);
        self
    }

    /// Build a [`MemoryStore`].
    ///
    /// # Panics
    ///
    /// Panics if configured with either `time_to_live` or `time_to_idle` higher than
    /// 1000 years. This is done to protect against overflow when computing key
    /// expiration.
    pub fn build(self) -> MemoryStore<K> {
        MemoryStore {
            inner: self.inner.build(),
        }
    }
}
/// A simple in-memory store for rate limiter.
pub struct MemoryStore<K> {
    inner: MokaCache<K, CachedEntry>,
}
impl<K> MemoryStore<K>
where
    K: Hash + Eq + Send + Sync + Clone + 'static,
{
    /// Create a new `MemoryStore`.
    pub fn new(max_capacity: u64) -> Self {
        Self {
            inner: MokaCache::new(max_capacity),
        }
    }
    
    /// Returns a [`Builder`], which can builds a `MemoryStore`
    pub fn builder() -> Builder<K> {
        Builder {
            inner: MokaCache::builder(),
        }
    }
}

#[async_trait]
impl<K> CacheStore for MemoryStore<K>
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
        self.inner.get(key)
    }

    async fn save_entry(&self, key: Self::Key, entry: CachedEntry) -> Result<(), Self::Error> {
        self.inner.insert(key, entry);
        Ok(())
    }
}
