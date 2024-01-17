use std::borrow::Borrow;
use std::convert::Infallible;
use std::hash::Hash;

use moka::future::Cache as MokaCache;

use super::{RateGuard, RateStore};

/// A simple in-memory store for rate limiter.
#[derive(Debug)]
pub struct MokaStore<K, G>
where
    K: Hash + Eq + Send + Sync + Clone + 'static,
    G: RateGuard,
{
    inner: MokaCache<K, G>,
}
impl<K, G> Default for MokaStore<K, G>
where
    K: Hash + Eq + Send + Sync + Clone + 'static,
    G: RateGuard,
{
    fn default() -> Self {
        Self::new()
    }
}
impl<K, G> MokaStore<K, G>
where
    K: Hash + Eq + Send + Sync + Clone + 'static,
    G: RateGuard,
{
    /// Create a new `MokaStore`.
    pub fn new() -> Self {
        Self {
            inner: MokaCache::new(u64::MAX),
        }
    }
}

impl<K, G> RateStore for MokaStore<K, G>
where
    K: Hash + Eq + Send + Sync + Clone + 'static,
    G: RateGuard,
{
    type Error = Infallible;
    type Key = K;
    type Guard = G;

    async fn load_guard<Q>(&self, key: &Q, refer: &Self::Guard) -> Result<Self::Guard, Self::Error>
    where
        Self::Key: Borrow<Q>,
        Q: Hash + Eq + Sync,
    {
        let guard = self.inner.get(key).await;
        if let Some(guard) = guard {
            Ok(guard)
        } else {
            Ok(refer.clone())
        }
    }

    async fn save_guard(&self, key: Self::Key, guard: Self::Guard) -> Result<(), Self::Error> {
        self.inner.insert(key, guard).await;
        Ok(())
    }
}
