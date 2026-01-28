use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::{BasicQuota, RateGuard};

/// Fixed window implement.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct FixedGuard {
    reset: OffsetDateTime,
    count: usize,
    quota: Option<BasicQuota>,
}

impl Default for FixedGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl FixedGuard {
    /// Create a new `FixedGuard`.
    #[must_use] pub fn new() -> Self {
        Self {
            reset: OffsetDateTime::now_utc(),
            count: 0,
            quota: None,
        }
    }
}

impl RateGuard for FixedGuard {
    type Quota = BasicQuota;
    async fn verify(&mut self, quota: &Self::Quota) -> bool {
        if self.quota.is_none() || OffsetDateTime::now_utc() > self.reset || self.quota.as_ref() != Some(quota) {
            if self.quota.as_ref() != Some(quota) {
                let mut quota = quota.clone();
                if quota.limit == 0 {
                    quota.limit = 1;
                }
                self.quota = Some(quota);
            }
            self.reset = OffsetDateTime::now_utc() + quota.period;
            self.count = 1;
            true
        } else if self.count < quota.limit {
            self.count += 1;
            true
        } else {
            false
        }
    }

    async fn remaining(&self, quota: &Self::Quota) -> usize {
        quota.limit - self.count
    }

    async fn reset(&self, _: &Self::Quota) -> i64 {
        self.reset.unix_timestamp()
    }

    async fn limit(&self, quota: &Self::Quota) -> usize {
        quota.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    #[test]
    fn test_fixed_guard_new() {
        let guard = FixedGuard::new();
        assert_eq!(guard.count, 0);
        assert!(guard.quota.is_none());
    }

    #[test]
    fn test_fixed_guard_default() {
        let guard = FixedGuard::default();
        assert_eq!(guard.count, 0);
        assert!(guard.quota.is_none());
    }

    #[test]
    fn test_fixed_guard_debug() {
        let guard = FixedGuard::new();
        let debug_str = format!("{:?}", guard);
        assert!(debug_str.contains("FixedGuard"));
        assert!(debug_str.contains("reset"));
        assert!(debug_str.contains("count"));
    }

    #[test]
    fn test_fixed_guard_clone() {
        let guard = FixedGuard::new();
        let cloned = guard.clone();
        assert_eq!(guard.count, cloned.count);
    }

    #[tokio::test]
    async fn test_fixed_guard_verify_first_request() {
        let mut guard = FixedGuard::new();
        let quota = BasicQuota::per_second(5);

        let result = guard.verify(&quota).await;
        assert!(result);
        assert_eq!(guard.count, 1);
        assert!(guard.quota.is_some());
    }

    #[tokio::test]
    async fn test_fixed_guard_verify_within_limit() {
        let mut guard = FixedGuard::new();
        let quota = BasicQuota::per_second(3);

        assert!(guard.verify(&quota).await);
        assert!(guard.verify(&quota).await);
        assert!(guard.verify(&quota).await);
        assert_eq!(guard.count, 3);
    }

    #[tokio::test]
    async fn test_fixed_guard_verify_exceeds_limit() {
        let mut guard = FixedGuard::new();
        let quota = BasicQuota::per_second(2);

        assert!(guard.verify(&quota).await);
        assert!(guard.verify(&quota).await);
        assert!(!guard.verify(&quota).await);
    }

    #[tokio::test]
    async fn test_fixed_guard_verify_reset_after_period() {
        let mut guard = FixedGuard::new();
        let quota = BasicQuota::new(2, Duration::milliseconds(100));

        assert!(guard.verify(&quota).await);
        assert!(guard.verify(&quota).await);
        assert!(!guard.verify(&quota).await);

        // Wait for period to expire
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

        // Should be allowed again
        assert!(guard.verify(&quota).await);
    }

    #[tokio::test]
    async fn test_fixed_guard_remaining() {
        let mut guard = FixedGuard::new();
        let quota = BasicQuota::per_second(5);

        guard.verify(&quota).await;
        assert_eq!(guard.remaining(&quota).await, 4);

        guard.verify(&quota).await;
        assert_eq!(guard.remaining(&quota).await, 3);
    }

    #[tokio::test]
    async fn test_fixed_guard_limit() {
        let guard = FixedGuard::new();
        let quota = BasicQuota::per_second(10);

        assert_eq!(guard.limit(&quota).await, 10);
    }

    #[tokio::test]
    async fn test_fixed_guard_reset_timestamp() {
        let mut guard = FixedGuard::new();
        let quota = BasicQuota::per_second(5);

        guard.verify(&quota).await;

        let reset_time = guard.reset(&quota).await;
        let now = OffsetDateTime::now_utc().unix_timestamp();

        // Reset time should be approximately 1 second from now
        assert!(reset_time > now);
        assert!(reset_time <= now + 2);
    }

    #[tokio::test]
    async fn test_fixed_guard_quota_change() {
        let mut guard = FixedGuard::new();
        let quota1 = BasicQuota::per_second(2);
        let quota2 = BasicQuota::per_second(5);

        assert!(guard.verify(&quota1).await);
        assert!(guard.verify(&quota1).await);
        assert!(!guard.verify(&quota1).await);

        // Change quota should reset the counter
        assert!(guard.verify(&quota2).await);
        assert_eq!(guard.count, 1);
    }

}
