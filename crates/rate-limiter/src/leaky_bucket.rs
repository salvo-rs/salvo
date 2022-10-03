
//TODO
use std::time::{Duration, Instant};

use salvo_core::async_trait;

use super::{RateStore, RateGuard, SimpleQuota};

#[derive(Clone, Debug)]
pub struct LeakyBucket {
    /// The number of requests allowed in the window.
    limit: usize,
    /// The duration of the window.
    window: Duration,
    /// The time at which the window resets.
    reset: Instant,
    /// The number of requests made in the window.
    count: usize,
}

impl LeakyBucket {
    pub fn new(limit: usize, window: Duration) -> Self {
        Self {
            limit,
            window,
            reset: Instant::now() + window,
            count: 0,
        }
    }
}

#[async_trait]
impl RateGuard for LeakyBucket {
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
