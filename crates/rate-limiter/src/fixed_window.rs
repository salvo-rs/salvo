
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct FixedWindow {
    limit: usize,
    window: Duration,
    reset: Instant,
    count: usize,
}

impl FixedWindow {
    pub fn new(limit: usize, window: Duration) -> Self {
        Self {
            limit,
            window,
            reset: Instant::now() + window,
            count: 0,
        }
    }
}

impl RateStrategy for FixedWindow {
    fn check(&mut self) -> bool {
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