
//TODO
use std::time::{Duration, Instant};

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
            reset: Instant::now() + window,
            count: 0,
        }
    }
}

impl RateStrategy for TokenBucket {
    fn allow(&mut self) -> bool {
        if Instant::now() > self.reset {
            self.reset = Instant::now() + self.window;
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