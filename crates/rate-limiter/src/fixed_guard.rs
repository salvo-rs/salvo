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
