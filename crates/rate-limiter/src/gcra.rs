
//TODO

use std::convert::Infallible;
use std::time::{Duration, Instant};

use salvo_core::async_trait;

use super::{Strategy, RateGuard};

#[derive(Clone, Debug)]
pub struct SlidingWindow {
    /// The number of requests allowed in the window.
    limit: usize,
    /// The duration of the window.
    window: Duration,
    /// The time at which the window resets.
    reset: Instant,
    /// The number of requests made in the window.
    count: usize,
}

impl SlidingWindow {
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
impl RateGuard for SlidingWindow {
    async fn pass(&mut self) -> bool {
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
