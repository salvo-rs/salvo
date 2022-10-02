
use std::time::{Duration, Instant};

pub struct FixedWindow {
    /// The number of requests allowed in the window.
    limit: usize,
    /// The duration of the window.
    window: Duration,
    /// The time at which the window resets.
    reset: Instant,
    /// The number of requests made in the window.
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
    type Error = Error;
    fn allow(&mut self) -> Result<bool, Self::Error> {
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