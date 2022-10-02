
//TODO
use std::time::{Duration, Instant};

use salvo_core::async_trait;

use super::{RateStore, RateStrategy};

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
impl RateStrategy for TokenBucket {
    async fn check(&mut self) -> bool {
        if Instant::now() > self.reset {
            self.reset = Instant::now() + self.period;
            self.count = 0;
        }
        if self.count < self.limit {
            self.count += 1;
            true
        } else {
            false
        }
    }
}