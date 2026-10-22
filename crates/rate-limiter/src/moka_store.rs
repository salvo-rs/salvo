use std::borrow::Borrow;
use std::convert::Infallible;
use std::hash::Hash;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use moka::Expiry;
use moka::future::Cache as MokaCache;
use moka::ops::compute;

use super::{RateGuard, RateLimitState, RateStore};

const DEFAULT_MAX_CAPACITY: u64 = 100_000;
const DEFAULT_TIME_TO_IDLE: Duration = Duration::from_secs(60 * 60);

/// A bounded, in-memory store for rate limiter state.
///
/// Guards verified through [`RateStore::verify_guard`] are retained until at
/// least their reported reset time, even when the idle duration is shorter.
/// Capacity eviction and process restarts can still discard active guards and
/// restore quota early. Use a persistent shared store for strict quotas across
/// restarts or multiple server processes.
#[derive(Debug)]
pub struct MokaStore<K, G>
where
    K: Hash + Eq + Send + Sync + Clone + 'static,
    G: RateGuard,
{
    inner: MokaCache<K, StoredGuard<G>>,
    time_to_idle: Option<Duration>,
}

#[derive(Clone, Debug)]
struct StoredGuard<G> {
    guard: G,
    retain_for: Duration,
}

#[derive(Debug)]
struct GuardExpiry {
    time_to_idle: Duration,
}

impl<K, G> Expiry<K, StoredGuard<G>> for GuardExpiry {
    fn expire_after_create(
        &self,
        _key: &K,
        value: &StoredGuard<G>,
        _at: Instant,
    ) -> Option<Duration> {
        Some(value.retain_for.max(self.time_to_idle))
    }

    fn expire_after_read(
        &self,
        _key: &K,
        _value: &StoredGuard<G>,
        _at: Instant,
        remaining: Option<Duration>,
        _modified_at: Instant,
    ) -> Option<Duration> {
        Some(remaining.unwrap_or_default().max(self.time_to_idle))
    }

    fn expire_after_update(
        &self,
        _key: &K,
        value: &StoredGuard<G>,
        _at: Instant,
        _remaining: Option<Duration>,
    ) -> Option<Duration> {
        Some(value.retain_for.max(self.time_to_idle))
    }
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
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity_and_time_to_idle(DEFAULT_MAX_CAPACITY, DEFAULT_TIME_TO_IDLE)
    }

    /// Create a new `MokaStore` with a maximum number of identifiers.
    #[must_use]
    pub fn with_max_capacity(max_capacity: u64) -> Self {
        Self::with_capacity_and_time_to_idle(max_capacity, DEFAULT_TIME_TO_IDLE)
    }

    /// Create a new `MokaStore` with a maximum number of identifiers and idle eviction.
    ///
    /// The idle duration is extended when necessary to retain guards until their
    /// reset time. The capacity limit can still evict guards before they reset.
    #[must_use]
    pub fn with_capacity_and_time_to_idle(max_capacity: u64, time_to_idle: Duration) -> Self {
        Self {
            inner: MokaCache::builder()
                .max_capacity(max_capacity)
                .expire_after(GuardExpiry { time_to_idle })
                .build(),
            time_to_idle: Some(time_to_idle),
        }
    }

    /// Create an unbounded `MokaStore`.
    ///
    /// Prefer [`MokaStore::new`] or [`MokaStore::with_max_capacity`] for public services where
    /// attackers can create high-cardinality identifiers.
    #[must_use]
    pub fn unbounded() -> Self {
        Self {
            inner: MokaCache::new(u64::MAX),
            time_to_idle: None,
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
                let mut guard = entry.map_or(refer, |entry| entry.into_value().guard);
                let verified = guard.verify(quota).await;
                // reset() is expressed in whole Unix seconds. Retain through
                // that whole second so rounding cannot expire a live window.
                let now = time::OffsetDateTime::now_utc().unix_timestamp();
                let seconds = guard
                    .reset(quota)
                    .await
                    .saturating_sub(now)
                    .saturating_add(1)
                    .max(0);
                let retain_for = Duration::from_secs(seconds as u64);
                allowed_in_compute.store(verified, Ordering::Relaxed);
                compute::Op::Put(StoredGuard { guard, retain_for })
            })
            .await;

        let Some(entry) = result.into_entry() else {
            unreachable!("MokaStore verify_guard always stores an updated guard");
        };
        Ok(RateLimitState {
            allowed: allowed.load(Ordering::Relaxed),
            guard: entry.into_value().guard,
        })
    }

    async fn load_guard<Q>(&self, key: &Q, refer: &Self::Guard) -> Result<Self::Guard, Self::Error>
    where
        Self::Key: Borrow<Q>,
        Q: Hash + Eq + Sync,
    {
        let guard = self.inner.get(key).await;
        if let Some(guard) = guard {
            Ok(guard.guard)
        } else {
            Ok(refer.clone())
        }
    }

    async fn save_guard(&self, key: Self::Key, guard: Self::Guard) -> Result<(), Self::Error> {
        // This low-level method has no quota from which to calculate reset;
        // quota enforcement should use verify_guard instead.
        self.inner
            .insert(
                key,
                StoredGuard {
                    guard,
                    retain_for: self.time_to_idle.unwrap_or_default(),
                },
            )
            .await;
        Ok(())
    }
}

#[cfg(all(test, feature = "fixed-guard"))]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::{BasicQuota, FixedGuard};

    #[tokio::test]
    async fn idle_eviction_keeps_an_unexpired_quota() {
        let store = MokaStore::<String, FixedGuard>::with_capacity_and_time_to_idle(
            100,
            Duration::from_millis(50),
        );
        let quota = BasicQuota::set_hours(1, 24);
        let refer = FixedGuard::new();
        assert!(
            store
                .verify_guard("client".into(), &refer, &quota)
                .await
                .unwrap()
                .allowed
        );
        assert!(
            !store
                .verify_guard("client".into(), &refer, &quota)
                .await
                .unwrap()
                .allowed
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            !store
                .verify_guard("client".into(), &refer, &quota)
                .await
                .unwrap()
                .allowed
        );
    }

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

    #[test]
    fn new_uses_bounded_cache_defaults() {
        let store = MokaStore::<String, FixedGuard>::new();
        assert_eq!(
            store.inner.policy().max_capacity(),
            Some(DEFAULT_MAX_CAPACITY)
        );
    }
}
