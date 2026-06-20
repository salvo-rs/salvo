use time::{Duration, OffsetDateTime};

use super::{CelledQuota, RateGuard};

/// Sliding-window rate limiter implementation.
///
/// The window is approximated with a ring of `cells` fixed-width buckets
/// (a "sliding window counter"). Requests are attributed to the bucket covering
/// the instant they arrive, and a bucket is evicted once a full `period` has
/// passed since it started. This keeps memory at `O(cells)` regardless of the
/// request rate, at the cost of sub-cell precision: every request inside a
/// bucket shares that bucket's start time, so a burst clustered near the end of
/// a bucket can be forgotten up to one `cell_span` early when the bucket is
/// reused. The error is bounded by a single bucket's worth of requests and
/// shrinks as `cells` grows (note `cells` is capped at `limit`). An exact
/// sliding window would require storing a timestamp per request.
#[derive(Clone, Debug)]
pub struct SlidingGuard {
    cell_start: OffsetDateTime,
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            cell_start: OffsetDateTime::now_utc(),
            cell_span: Duration::default(),
            counts: vec![],
            head: 0,
            total: 0,
            quota: None,
        }
    }

    fn reset_window(&mut self, quota: &CelledQuota, now: OffsetDateTime) {
        self.cell_start = now;
        self.cell_span = Self::cell_span(quota);
        self.counts = vec![0; quota.cells];
        self.head = 0;
        self.counts[0] = 1;
        self.total = 1;
    }

    /// Width of a single cell, rounded **up** so the whole ring spans at least
    /// `quota.period`. With truncating division `cells * cell_span` can be
    /// shorter than `period`, which would let the head wrap around and evict a
    /// still-valid request before `period` elapsed.
    fn cell_span(quota: &CelledQuota) -> Duration {
        let cells = quota.cells.max(1) as u32;
        let span = quota.period / cells;
        if span * cells < quota.period {
            span + Duration::nanoseconds(1)
        } else {
            span
        }
    }
}

impl RateGuard for SlidingGuard {
    type Quota = CelledQuota;
    async fn verify(&mut self, quota: &Self::Quota) -> bool {
        let now = OffsetDateTime::now_utc();
        let quota = quota.normalized();
        if self.quota.as_ref() != Some(&quota) {
            self.reset_window(&quota, now);
            self.quota = Some(quota);
            return true;
        }
        let mut delta = now - self.cell_start;
        if delta >= quota.period {
            self.reset_window(&quota, now);
            return true;
        }
        // Advance the head over every whole cell span that has elapsed since the
        // current cell started, evicting the counts of the cells that slide out
        // of the window. Cell boundaries are anchored in absolute time, so
        // `cell_start` is moved forward by whole cell spans instead of being
        // reset to `now` on every request (which would freeze the window).
        while delta >= self.cell_span {
            delta -= self.cell_span;
            self.head = (self.head + 1) % self.counts.len();
            self.total = self.total.saturating_sub(self.counts[self.head]);
            self.counts[self.head] = 0;
        }
        self.counts[self.head] += 1;
        self.total += 1;
        self.cell_start = now - delta;
        self.total <= quota.limit
    }

    async fn remaining(&self, quota: &Self::Quota) -> usize {
        let quota = quota.normalized();
        quota.limit.saturating_sub(self.total)
    }

    async fn reset(&self, quota: &Self::Quota) -> i64 {
        let quota = quota.normalized();
        (self.cell_start + quota.period).unix_timestamp()
    }

    async fn limit(&self, quota: &Self::Quota) -> usize {
        let quota = quota.normalized();
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
        assert!(debug_str.contains("cell_start"));
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
    async fn test_sliding_guard_verify_zero_limit() {
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::per_second(0, 2);

        assert!(guard.verify(&quota).await);
        assert!(!guard.verify(&quota).await);
        assert_eq!(guard.limit(&quota).await, 1);
        assert_eq!(guard.remaining(&quota).await, 0);
    }

    #[tokio::test]
    async fn test_sliding_guard_normalizes_tiny_cell_span() {
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::new(10, 10, Duration::nanoseconds(1));

        assert!(guard.verify(&quota).await);
        assert_eq!(guard.counts.len(), 1);
        assert!(guard.cell_span > Duration::ZERO);
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
    async fn test_sliding_guard_ring_span_covers_period() {
        // Regression test: `period / cells` truncates, so the ring
        // (`cells * cell_span`) could span slightly less than `period`. When it
        // does, advancing over a near-full period wraps the head all the way
        // around and evicts a request that is still inside the window. The ring
        // must always cover at least the full period.
        let mut guard = SlidingGuard::new();
        let quota = CelledQuota::new(9, 3, Duration::seconds(1)); // 1s / 3 is not exact
        assert!(guard.verify(&quota).await);

        let normalized = quota.normalized();
        let ring = guard.cell_span * (normalized.cells as u32);
        assert!(
            ring >= normalized.period,
            "ring span {ring:?} shorter than period {:?}",
            normalized.period
        );
    }

    #[tokio::test]
    async fn test_sliding_guard_window_slides_with_small_gaps() {
        // Regression test: cell boundaries must be anchored in absolute time.
        // With several requests spaced closer than one cell span, but whose
        // cumulative elapsed time exceeds a cell span, the head must still
        // advance. The previous implementation reset `cell_start` to `now` on
        // every request, so `delta` only ever measured the inter-request gap
        // and the window never slid (head stayed at 0 forever).
        let mut guard = SlidingGuard::new();
        // limit is high so requests are never rejected and cannot interfere.
        let quota = CelledQuota::new(100, 3, Duration::milliseconds(300)); // cell_span = 100ms

        assert!(guard.verify(&quota).await);
        for _ in 0..4 {
            tokio::time::sleep(tokio::time::Duration::from_millis(45)).await;
            assert!(guard.verify(&quota).await);
        }

        assert!(
            guard.head > 0,
            "sliding window head did not advance over time, head={}",
            guard.head
        );
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
            guard.cell_start = OffsetDateTime::now_utc() - Duration::seconds(3);
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
