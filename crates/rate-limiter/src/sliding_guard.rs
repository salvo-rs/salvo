//TODO

use std::convert::Infallible;
use std::time::{Duration, Instant};

use salvo_core::async_trait;

use super::{CelledQuota, RateGuard, RateStore};

#[derive(Clone, Debug)]
pub struct SlidingGuard {
    cell_inst: Instant,
    cell_span: Duration,
    counts: Vec<usize>,
    head: usize,
    quota: Option<CelledQuota>,
}

impl Default for SlidingGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl SlidingGuard {
    pub fn new() -> Self {
        Self {
            cell_inst: Instant::now(),
            cell_span: Duration::default(),
            counts: vec![],
            head: 0,
            quota: None,
        }
    }
}

#[async_trait]
impl RateGuard for SlidingGuard {
    type Quota = CelledQuota;
    async fn verify(&mut self, quota: &Self::Quota) -> bool {
        if self.quota.is_none() || self.quota.as_ref() != Some(quota) {
            let mut quota = quota.clone();
            if quota.limit <= 0 {
                quota.limit = 1;
            }
            if quota.cells <= 0 {
                quota.cells = 1;
            }
            self.cell_inst = Instant::now();
            self.cell_span = quota.period / (quota.cells as u32);
            self.counts = vec![0; quota.cells];
            self.head = 0;
            self.counts[0] = 1;
            self.quota = Some(quota);
            true
        } else {
            if self.counts.iter().map(|v|*v).sum::<usize>() >= quota.limit {
                false
            } else {
                if Instant::now() > self.cell_inst + self.cell_span {
                    self.cell_inst = Instant::now();
                    self.head = (self.head + 1) % self.counts.len();
                }
                self.counts[self.head] += 1;
                true
            }
        }
    }
}
