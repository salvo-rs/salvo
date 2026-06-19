use std::borrow::Borrow;
use std::convert::Infallible;
use std::error::Error as StdError;
use std::hash::Hash;

use serde::{Deserialize, Serialize};
use time::Duration;

/// Used to get quota settings, for example from a database.
pub trait QuotaGetter<Key>: Send + Sync + 'static {
    /// Quota type.
    type Quota: Clone + Send + Sync + 'static;
    /// Error type.
    type Error: StdError;

    /// Get quota.
    fn get<Q>(&self, key: &Q) -> impl Future<Output = Result<Self::Quota, Self::Error>> + Send
    where
        Key: Borrow<Q>,
        Q: Hash + Eq + Sync;
}

/// A basic quota.
#[non_exhaustive]
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug)]
pub struct BasicQuota {
    /// The limit of requests.
    pub limit: usize,
    /// The period of requests.
    pub period: Duration,
}
impl BasicQuota {
    /// Creates a new `BasicQuota`.
    #[must_use]
    pub const fn new(limit: usize, period: Duration) -> Self {
        Self { limit, period }
    }

    /// Sets the limit of the quota per second.
    #[must_use]
    pub const fn per_second(limit: usize) -> Self {
        Self::new(limit, Duration::seconds(1))
    }
    /// Creates a quota with a custom number of seconds.
    #[must_use]
    pub const fn set_seconds(limit: usize, seconds: i64) -> Self {
        Self::new(limit, Duration::seconds(seconds))
    }

    /// Sets the limit of the quota per minute.
    #[must_use]
    pub const fn per_minute(limit: usize) -> Self {
        Self::new(limit, Duration::seconds(60))
    }
    /// Creates a quota with a custom number of minutes.
    #[must_use]
    pub const fn set_minutes(limit: usize, minutes: i64) -> Self {
        Self::new(limit, Duration::seconds(60 * minutes))
    }

    /// Sets the limit of the quota per hour.
    #[must_use]
    pub const fn per_hour(limit: usize) -> Self {
        Self::new(limit, Duration::seconds(3600))
    }
    /// Creates a quota with a custom number of hours.
    #[must_use]
    pub const fn set_hours(limit: usize, hours: i64) -> Self {
        Self::new(limit, Duration::seconds(3600 * hours))
    }

    pub(crate) fn normalized(&self) -> Self {
        let mut quota = self.clone();
        quota.limit = quota.limit.max(1);
        if quota.period <= Duration::ZERO {
            quota.period = Duration::seconds(1);
        }
        quota
    }
}

/// A quota split into cells for sliding-window accounting.
#[non_exhaustive]
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug)]
pub struct CelledQuota {
    /// The limit of requests.
    pub limit: usize,
    /// The period of requests.
    pub period: Duration,
    /// The number of cells the period is split into.
    pub cells: usize,
}
impl CelledQuota {
    /// Creates a new `CelledQuota`.
    #[must_use]
    pub const fn new(limit: usize, cells: usize, period: Duration) -> Self {
        Self {
            limit,
            cells,
            period,
        }
    }

    /// Sets the limit of the quota per second.
    #[must_use]
    pub const fn per_second(limit: usize, cells: usize) -> Self {
        Self::new(limit, cells, Duration::seconds(1))
    }
    /// Creates a quota with a custom number of seconds.
    #[must_use]
    pub const fn set_seconds(limit: usize, cells: usize, seconds: i64) -> Self {
        Self::new(limit, cells, Duration::seconds(seconds))
    }

    /// Sets the limit of the quota per minute.
    #[must_use]
    pub const fn per_minute(limit: usize, cells: usize) -> Self {
        Self::new(limit, cells, Duration::seconds(60))
    }
    /// Creates a quota with a custom number of minutes.
    #[must_use]
    pub const fn set_minutes(limit: usize, cells: usize, minutes: i64) -> Self {
        Self::new(limit, cells, Duration::seconds(60 * minutes))
    }

    /// Sets the limit of the quota per hour.
    #[must_use]
    pub const fn per_hour(limit: usize, cells: usize) -> Self {
        Self::new(limit, cells, Duration::seconds(3600))
    }
    /// Creates a quota with a custom number of hours.
    #[must_use]
    pub const fn set_hours(limit: usize, cells: usize, hours: i64) -> Self {
        Self::new(limit, cells, Duration::seconds(3600 * hours))
    }

    pub(crate) fn normalized(&self) -> Self {
        let mut quota = self.clone();
        quota.limit = quota.limit.max(1);
        if quota.period <= Duration::ZERO {
            quota.period = Duration::seconds(1);
        }
        quota.cells = quota.cells.max(1).min(quota.limit).min(u32::MAX as usize);

        let max_nonzero_cells = usize::try_from(quota.period.whole_nanoseconds())
            .unwrap_or(usize::MAX)
            .max(1);
        quota.cells = quota.cells.min(max_nonzero_cells);
        quota
    }
}

impl<Key, T> QuotaGetter<Key> for T
where
    Key: Hash + Eq + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    type Quota = T;
    type Error = Infallible;

    async fn get<Q>(&self, _key: &Q) -> Result<Self::Quota, Self::Error>
    where
        Key: Borrow<Q>,
        Q: Hash + Eq + Sync,
    {
        Ok(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_quota() {
        let quota = BasicQuota::per_second(10);
        assert_eq!(quota.limit, 10);
        assert_eq!(quota.period, Duration::seconds(1));

        let quota = BasicQuota::set_seconds(15, 2);
        assert_eq!(quota.limit, 15);
        assert_eq!(quota.period, Duration::seconds(2));

        let quota = BasicQuota::per_minute(10);
        assert_eq!(quota.limit, 10);
        assert_eq!(quota.period, Duration::seconds(60));

        let quota = BasicQuota::set_minutes(15, 2);
        assert_eq!(quota.limit, 15);
        assert_eq!(quota.period, Duration::seconds(120));

        let quota = BasicQuota::per_hour(10);
        assert_eq!(quota.limit, 10);
        assert_eq!(quota.period, Duration::seconds(3600));

        let quota = BasicQuota::set_hours(15, 2);
        assert_eq!(quota.limit, 15);
        assert_eq!(quota.period, Duration::seconds(7200));
    }

    #[test]
    fn test_basic_quota_normalized() {
        let quota = BasicQuota::set_seconds(0, 0).normalized();

        assert_eq!(quota.limit, 1);
        assert_eq!(quota.period, Duration::seconds(1));
    }

    #[test]
    fn test_celled_quota() {
        let quota = CelledQuota::per_second(10, 3);
        assert_eq!(quota.limit, 10);
        assert_eq!(quota.cells, 3);
        assert_eq!(quota.period, Duration::seconds(1));

        let quota = CelledQuota::set_seconds(15, 7, 2);
        assert_eq!(quota.limit, 15);
        assert_eq!(quota.cells, 7);
        assert_eq!(quota.period, Duration::seconds(2));

        let quota = CelledQuota::per_minute(10, 9);
        assert_eq!(quota.limit, 10);
        assert_eq!(quota.cells, 9);
        assert_eq!(quota.period, Duration::seconds(60));

        let quota = CelledQuota::set_minutes(15, 7, 2);
        assert_eq!(quota.limit, 15);
        assert_eq!(quota.cells, 7);
        assert_eq!(quota.period, Duration::seconds(120));

        let quota = CelledQuota::per_hour(10, 3);
        assert_eq!(quota.limit, 10);
        assert_eq!(quota.cells, 3);
        assert_eq!(quota.period, Duration::seconds(3600));

        let quota = CelledQuota::set_hours(15, 6, 2);
        assert_eq!(quota.limit, 15);
        assert_eq!(quota.cells, 6);
        assert_eq!(quota.period, Duration::seconds(7200));
    }

    #[test]
    fn test_celled_quota_normalized() {
        let quota = CelledQuota::new(0, 0, Duration::seconds(0)).normalized();

        assert_eq!(quota.limit, 1);
        assert_eq!(quota.cells, 1);
        assert_eq!(quota.period, Duration::seconds(1));

        let quota = CelledQuota::new(10, 10, Duration::nanoseconds(1)).normalized();
        assert_eq!(quota.cells, 1);
    }
}
