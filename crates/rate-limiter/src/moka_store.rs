use std::borrow::Borrow;
use std::convert::Infallible;
use std::hash::Hash;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use moka::future::Cache as MokaCache;
use moka::ops::compute;

use super::{RateGuard, RateLimitState, RateStore};

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
    #[must_use] pub fn new() -> Self {
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

    async fn verify_guard(
        &self,
        key: Self::Key,
        refer: &Self::Guard,
        quota: &<Self::Guard as RateGuard>::Quota,
    ) -> Result<RateLimitState<Self::Guard>, Self::Error> {
        let allowed = Arc::new(AtomicBool::new(false));
        let allowed_in_compute = Arc::clone(&allowed);
        let refer = refer.clone();

        let result = self
            .inner
            .entry(key)
            .and_compute_with(|entry| async move {
                let mut guard = entry.map_or(refer, |entry| entry.into_value());
                let verified = guard.verify(quota).await;
                allowed_in_compute.store(verified, Ordering::Relaxed);
                compute::Op::Put(guard)
            })
            .await;

        let Some(entry) = result.into_entry() else {
            unreachable!("MokaStore verify_guard always stores an updated guard");
        };
        Ok(RateLimitState {
            allowed: allowed.load(Ordering::Relaxed),
            guard: entry.into_value(),
        })
    }

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

#[cfg(all(test, feature = "fixed-guard"))]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::{BasicQuota, FixedGuard};

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn verify_guard_enforces_fixed_limit_concurrently() {
        let store = Arc::new(MokaStore::<String, FixedGuard>::default());
        let quota = Arc::new(BasicQuota::per_second(10));
        let refer = Arc::new(FixedGuard::new());
        let allowed_count = Arc::new(AtomicUsize::new(0));
        let key = "shared_client".to_owned();

        let mut handles = Vec::new();
        for _ in 0..30 {
            let store = Arc::clone(&store);
            let quota = Arc::clone(&quota);
            let refer = Arc::clone(&refer);
            let allowed_count = Arc::clone(&allowed_count);
            let key = key.clone();

            handles.push(tokio::spawn(async move {
                let state = store.verify_guard(key, &refer, &quota).await.unwrap();
                if state.allowed {
                    allowed_count.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(allowed_count.load(Ordering::SeqCst), 10);
    }
}
