use time::{Duration, OffsetDateTime};

use super::{CelledQuota, RateGuard};

/// Sliding window implement.
#[derive(Clone, Debug)]
pub struct SlidingGuard {
    cell_inst: OffsetDateTime,
    cell_span: Duration,
    counts: Vec<usize>,
    head: usize,
    total: usize,
    quota: Option<CelledQuota>,
}

impl Default for SlidingGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl SlidingGuard {
    /// Create a new `SlidingGuard`.
    #[must_use] pub fn new() -> Self {
        Self {
            cell_inst: OffsetDateTime::now_utc(),
            cell_span: Duration::default(),
            counts: vec![],
            head: 0,
            total: 0,
            quota: None,
        }
    }

    fn normalize_quota(quota: &CelledQuota) -> CelledQuota {
        let mut quota = quota.clone();
        if quota.limit == 0 {
            quota.limit = 1;
        }
        if quota.cells == 0 {
            quota.cells = 1;
        }
        if quota.cells > quota.limit {
            quota.cells = quota.limit;
        }
        quota
    }

    fn reset_window(&mut self, quota: &CelledQuota, now: OffsetDateTime) {
        self.cell_inst = now;
        self.cell_span = quota.period / (quota.cells as u32);
        self.counts = vec![0; quota.cells];
        self.head = 0;
        self.counts[0] = 1;
        self.total = 1;
    }
}

impl RateGuard for SlidingGuard {
    type Quota = CelledQuota;
    async fn verify(&mut self, quota: &Self::Quota) -> bool {
        let now = OffsetDateTime::now_utc();
        let quota = Self::normalize_quota(quota);
        if self.quota.as_ref() != Some(&quota) {
            self.reset_window(&quota, now);
            self.quota = Some(quota);
            return true;
        }
        let mut delta = now - self.cell_inst;
        if delta > quota.period {
            self.reset_window(&quota, now);
            return true;
        } else {
            while delta > self.cell_span {
                delta -= self.cell_span;
                self.head = (self.head + 1) % self.counts.len();
                self.total = self.total.saturating_sub(self.counts[self.head]);
                self.counts[self.head] = 0;
            }
            self.counts[self.head] += 1;
            self.total += 1;
            self.cell_inst = now;
        }
        self.total <= quota.limit
    }

    async fn remaining(&self, quota: &Self::Quota) -> usize {
        quota.limit.saturating_sub(self.total)
    }

    async fn reset(&self, quota: &Self::Quota) -> i64 {
        (self.cell_inst + quota.period).unix_timestamp()
    }

    async fn limit(&self, quota: &Self::Quota) -> usize {
        quota.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sliding_guard_new() {
        let guard = SlidingGuard::new();
        assert!(guard.counts.is_empty());
        assert_eq!(guard.head, 0);
        assert_eq!(guard.total, 0);
        assert!(guard.quota.is_none());
    }

    #[test]
    fn test_sliding_guard_default() {
        let guard = SlidingGuard::default();
        assert!(guard.counts.is_empty());
        assert_eq!(guard.head, 0);
        assert_eq!(guard.total, 0);
        assert!(guard.quota.is_none());
    }

    #[test]
    fn test_sliding_guard_debug() {
        let guard = SlidingGuard::new();
        let debug_str = format!("{guard:?}");
        assert!(debug_str.contains("SlidingGuard"));
        assert!(debug_str.contains("cell_inst"));
        assert!(debug_str.contains("counts"));
    }

    #[test]
    fn test_sliding_guard_clone() {
        let guard = SlidingGuard::new();
        let cloned = guard.clone();
        assert_eq!(guard.head, cloned.head);
        assert_eq!(guard.counts, cloned.counts);
        assert_eq!(guard.total, cloned.total);
    }

    #[tokio::test]
    async fn test_sliding_guard_verify_first_request() {
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::per_second(5, 2);

        let result = guard.verify(&quota).await;
        assert!(result);
        assert!(!guard.counts.is_empty());
        assert_eq!(guard.total, 1);
        assert!(guard.quota.is_some());
    }

    #[tokio::test]
    async fn test_sliding_guard_verify_within_limit() {
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::per_second(3, 2);

        assert!(guard.verify(&quota).await);
        assert!(guard.verify(&quota).await);
        assert!(guard.verify(&quota).await);
    }

    #[tokio::test]
    async fn test_sliding_guard_verify_exceeds_limit() {
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::per_second(2, 2);

        assert!(guard.verify(&quota).await);
        assert!(guard.verify(&quota).await);
        assert!(!guard.verify(&quota).await);
    }

    #[tokio::test]
    async fn test_sliding_guard_verify_zero_cells() {
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::new(5, 0, Duration::seconds(1));

        // Zero cells should be treated as 1
        assert!(guard.verify(&quota).await);
        assert_eq!(guard.counts.len(), 1);
    }

    #[tokio::test]
    async fn test_sliding_guard_verify_cells_greater_than_limit() {
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::new(3, 10, Duration::seconds(1));

        // Cells should be clamped to limit
        assert!(guard.verify(&quota).await);
        assert_eq!(guard.counts.len(), 3);
    }

    #[tokio::test]
    async fn test_sliding_guard_clamped_cells_keep_counting() {
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::new(3, 10, Duration::seconds(1));

        assert!(guard.verify(&quota).await);
        assert!(guard.verify(&quota).await);
        assert!(guard.verify(&quota).await);
        assert!(!guard.verify(&quota).await);
        assert_eq!(guard.counts.len(), 3);
        assert_eq!(guard.total, guard.counts.iter().sum::<usize>());
    }

    #[tokio::test]
    async fn test_sliding_guard_verify_reset_after_period() {
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::new(2, 2, Duration::milliseconds(100));

        assert!(guard.verify(&quota).await);
        assert!(guard.verify(&quota).await);
        assert!(!guard.verify(&quota).await);

        // Wait for period to expire
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

        // Should be allowed again
        assert!(guard.verify(&quota).await);
    }

    #[tokio::test]
    async fn test_sliding_guard_remaining() {
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::per_second(5, 2);

        guard.verify(&quota).await;
        let remaining = guard.remaining(&quota).await;
        assert!(remaining < 5);
        assert_eq!(remaining, 4);

        guard.verify(&quota).await;
        let remaining2 = guard.remaining(&quota).await;
        assert!(remaining2 < remaining);
        assert_eq!(remaining2, 3);
    }

    #[tokio::test]
    async fn test_sliding_guard_remaining_saturating() {
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::per_second(2, 2);

        guard.verify(&quota).await;
        guard.verify(&quota).await;
        guard.verify(&quota).await;

        // Should not underflow
        let remaining = guard.remaining(&quota).await;
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn test_sliding_guard_limit() {
        let guard = SlidingGuard::new();
        let quota = CelledQuota::per_second(10, 3);

        assert_eq!(guard.limit(&quota).await, 10);
    }

    #[tokio::test]
    async fn test_sliding_guard_reset_timestamp() {
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::per_second(5, 2);

        guard.verify(&quota).await;

        let reset_time = guard.reset(&quota).await;
        let now = OffsetDateTime::now_utc().unix_timestamp();

        // Reset time should be approximately 1 second from now
        assert!(reset_time > now);
        assert!(reset_time <= now + 2);
    }

    #[tokio::test]
    async fn test_sliding_guard_quota_change() {
        let mut guard = SlidingGuard::new();
        let quota1 = CelledQuota::per_second(2, 2);
        let quota2 = CelledQuota::per_second(5, 3);

        assert!(guard.verify(&quota1).await);
        assert!(guard.verify(&quota1).await);
        assert!(!guard.verify(&quota1).await);

        // Change quota should reset
        assert!(guard.verify(&quota2).await);
        assert_eq!(guard.counts.len(), 3);
    }

    #[tokio::test]
    async fn test_sliding_guard_multiple_cells() {
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::new(10, 5, Duration::seconds(1));

        guard.verify(&quota).await;

        assert_eq!(guard.counts.len(), 5);
    }

    #[tokio::test]
    async fn test_sliding_guard_fixed_interval_should_not_reject() {
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::new(4, 4, Duration::seconds(10));

        assert!(guard.verify(&quota).await);

        for request_index in 2..=12 {
            guard.cell_inst = OffsetDateTime::now_utc() - Duration::seconds(3);
            assert!(
                guard.verify(&quota).await,
                "request {request_index} rejected, head={}, counts={:?}",
                guard.head,
                guard.counts
            );
            assert_eq!(guard.total, guard.counts.iter().sum::<usize>());
        }
    }
}
