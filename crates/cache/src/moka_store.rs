//! Memory store module.
use std::borrow::Borrow;
use std::convert::Infallible;
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
    /// A cached entry will expire after the specified duration has passed since `get`
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
    /// A cached entry will expire after the specified duration has passed since
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

    /// Sets the eviction listener closure to the cache.
    ///
    /// # Panics
    ///
    /// It is very important to ensure the listener closure does not panic. Otherwise,
    /// the cache will stop calling the listener after a panic. This is intended
    /// behavior because the cache cannot know whether it is memory safe to
    /// call the panicked listener again.
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
    pub fn build(self) -> MokaStore<K> {
        MokaStore {
            inner: self.inner.build(),
        }
    }
}
/// A simple in-memory store for rate limiter.
pub struct MokaStore<K> {
    inner: MokaCache<K, CachedEntry>,
}
impl<K> MokaStore<K>
where
    K: Hash + Eq + Send + Sync + Clone + 'static,
{
    /// Create a new `MokaStore`.
    pub fn new(max_capacity: u64) -> Self {
        Self {
            inner: MokaCache::new(max_capacity),
        }
    }

    /// Returns a [`Builder`], which can build a `MokaStore`.
    pub fn builder() -> Builder<K> {
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
