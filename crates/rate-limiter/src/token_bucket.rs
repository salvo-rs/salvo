
//TODO
use std::time::{Duration, Instant};

use salvo_core::async_trait;

use super::{RateStore, RateGuard, SimpleQuota};

#[derive(Clone, Debug)]
pub struct TokenBucket {
    limit: usize,
    period: Duration,
    reset: Instant,
    count: usize,
}

impl TokenBucket {
    pub fn new(limit: usize, period: Duration) -> Self {
        Self {
            limit,
            period,
            reset: Instant::now() + period,
            count: 0,
        }
    }
}

#[async_trait]
impl RateGuard for TokenBucket {
    type Quota = SimpleQuota;
    async fn verify(&mut self, quota: &Self::Quota) -> bool {
        if Instant::now() > self.reset {
            self.reset = Instant::now() + quota.period;
            self.count = 0;
        }
        if self.count < quota.burst {
            self.count += 1;
            true
        } else {
            false
        }
    }
}