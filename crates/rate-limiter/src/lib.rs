//! Rate limiter middleware for Salvo.
//! 
//! Rate Limiter middleware is used to limiting the amount of requests to the server 
//! from a particular IP or id within a time period.
//! 
//! [`RateIssuer`] is used to issue a key to request, your can define your custom `RateIssuer`.
//! If you want just identify user by IP address, you can use [`RemoteIpIssuer`].
//! 
//! [`QuotaGetter`] is used to get quota for every key.
//! 
//! [`RateGuard`] is strategy to verify is the request exceeded quota.
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::borrow::Borrow;
use std::convert::Infallible;
use std::error::Error as StdError;
use std::hash::Hash;

use salvo_core::addr::SocketAddr;
use salvo_core::handler::{NoneSkipper, Skipper};
use salvo_core::http::{Request, Response, StatusCode, StatusError};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};
use serde::{Deserialize, Serialize};
use time::Duration;

#[macro_use]
mod cfg;

cfg_feature! {
    #![feature = "memory-store"]

    mod memory_store;
    pub use memory_store::MemoryStore;
}

cfg_feature! {
    #![feature = "fixed-guard"]

    mod fixed_guard;
    pub use fixed_guard::FixedGuard;
}

cfg_feature! {
    #![feature = "sliding-guard"]

    mod sliding_guard;
    pub use sliding_guard::SlidingGuard;
}

/// `QuotaGetter` is used to get quota. You can config users' quota config in database.
#[async_trait]
pub trait QuotaGetter<Key>: Send + Sync + 'static {
    /// Quota type.
    type Quota: Clone + Send + Sync + 'static;
    /// Error type.
    type Error: StdError;

    /// Get quota.
    async fn get<Q>(&self, key: &Q) -> Result<Self::Quota, Self::Error>
    where
        Key: Borrow<Q>,
        Q: Hash + Eq + Sync;
}

/// A basic quota.
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug)]
pub struct BasicQuota {
    limit: usize,
    period: Duration,
}
impl BasicQuota {
    /// Create new `BasicQuota`.
    pub const fn new(limit: usize, period: Duration) -> Self {
        Self { limit, period }
    }

    /// Sets the limit of the quota per second.
    pub const fn per_second(limit: usize) -> Self {
        Self::new(limit, Duration::seconds(1))
    }

    /// Sets the limit of the quota per minute.
    pub const fn per_minute(limit: usize) -> Self {
        Self::new(limit, Duration::seconds(60))
    }

    /// Sets the limit of the quota per hour.
    pub const fn per_hour(limit: usize) -> Self {
        Self::new(limit, Duration::seconds(3600))
    }
}

/// A common used quota has cells field.
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug)]
pub struct CelledQuota {
    limit: usize,
    period: Duration,
    cells: usize,
}
impl CelledQuota {
    /// Create new `CelledQuota`.
    pub const fn new(limit: usize, period: Duration, cells: usize) -> Self {
        Self { limit, period, cells }
    }

    /// Sets the limit of the quota per second.
    pub const fn per_second(limit: usize, cells: usize) -> Self {
        Self::new(limit, Duration::seconds(1), cells)
    }
    /// Sets the limit of the quota per minute.
    pub const fn per_minute(limit: usize, cells: usize) -> Self {
        Self::new(limit, Duration::seconds(60), cells)
    }
    /// Sets the limit of the quota per hour.
    pub const fn per_hour(limit: usize, cells: usize) -> Self {
        Self::new(limit, Duration::seconds(3600), cells)
    }
}

#[async_trait]
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

/// Issuer is used to identify every request.
#[async_trait]
pub trait RateIssuer: Send + Sync + 'static {
    /// The key is used to identify the rate limit.
    type Key: Hash + Eq + Send + Sync + 'static;
    /// Issue a new key for the request.
    async fn issue(&self, req: &mut Request, depot: &Depot) -> Option<Self::Key>;
}
#[async_trait]
impl<F, K> RateIssuer for F
where
    F: Fn(&mut Request, &Depot) -> Option<K> + Send + Sync + 'static,
    K: Hash + Eq + Send + Sync + 'static,
{
    type Key = K;
    async fn issue(&self, req: &mut Request, depot: &Depot) -> Option<Self::Key> {
        (self)(req, depot)
    }
}

/// Identify user by IP address.
pub struct RemoteIpIssuer;
#[async_trait]
impl RateIssuer for RemoteIpIssuer {
    type Key = String;
    async fn issue(&self, req: &mut Request, _depot: &Depot) -> Option<Self::Key> {
        match req.remote_addr() {
            Some(SocketAddr::IPv4(addr)) => Some(addr.ip().to_string()),
            Some(SocketAddr::IPv6(addr)) => Some(addr.ip().to_string()),
            Some(_) => None,
            None => None,
        }
    }
}

/// `RateGuard` is strategy to verify is the request exceeded quota
#[async_trait]
pub trait RateGuard: Clone + Send + Sync + 'static {
    /// The quota for the rate limit.
    type Quota: Clone + Send + Sync + 'static;
    /// Verify is current request exceed the quota.
    async fn verify(&mut self, quota: &Self::Quota) -> bool;
}

/// `RateStore` is used to store rate limit data.
#[async_trait]
pub trait RateStore: Send + Sync + 'static {
    /// Error type for RateStore.
    type Error: StdError;
    /// Key
    type Key: Hash + Eq + Send + Clone + 'static;
    /// Saved guard.
    type Guard;
    /// Get the guard from the store.
    async fn load_guard<Q>(&self, key: &Q, refer: &Self::Guard) -> Result<Self::Guard, Self::Error>
    where
        Self::Key: Borrow<Q>,
        Q: Hash + Eq + Sync;
    /// Save the guard from the store.
    async fn save_guard(&self, key: Self::Key, guard: Self::Guard) -> Result<(), Self::Error>;
}

/// `RateLimiter` is the main struct to used limit user request.
pub struct RateLimiter<G, S, I, Q> {
    guard: G,
    store: S,
    issuer: I,
    quota_getter: Q,
    skipper: Box<dyn Skipper>,
}

impl<G: RateGuard, S: RateStore, I: RateIssuer, P: QuotaGetter<I::Key>> RateLimiter<G, S, I, P> {
    /// Create a new `RateLimiter`
    #[inline]
    pub fn new(guard: G, store: S, issuer: I, quota_getter: P) -> Self {
        Self {
            guard,
            store,
            issuer,
            quota_getter,
            skipper: Box::new(NoneSkipper),
        }
    }

    /// Sets skipper and returns new `RateLimiter`.
    #[inline]
    pub fn with_skipper(mut self, skipper: impl Skipper) -> Self {
        self.skipper = Box::new(skipper);
        self
    }
}

#[async_trait]
impl<G, S, I, P> Handler for RateLimiter<G, S, I, P>
where
    G: RateGuard<Quota = P::Quota>,
    S: RateStore<Key = I::Key, Guard = G>,
    P: QuotaGetter<I::Key>,
    I: RateIssuer,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if self.skipper.skipped(req, depot) {
            return;
        }
        let key = match self.issuer.issue(req, depot).await {
            Some(key) => key,
            None => {
                res.set_status_error(StatusError::bad_request().with_detail("invalid identifier"));
                ctrl.skip_rest();
                return;
            }
        };
        let quota = match self.quota_getter.get(&key).await {
            Ok(quota) => quota,
            Err(e) => {
                tracing::error!(error = ?e, "RateLimiter error");
                res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
                ctrl.skip_rest();
                return;
            }
        };
        let mut guard = match self.store.load_guard(&key, &self.guard).await {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!(error = ?e, "RateLimiter error");
                res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
                ctrl.skip_rest();
                return;
            }
        };
        let verified = guard.verify(&quota).await;
        if !verified {
            res.set_status_code(StatusCode::TOO_MANY_REQUESTS);
            ctrl.skip_rest();
        }
        if let Err(e) = self.store.save_guard(key, guard).await {
            tracing::error!(error = ?e, "RateLimiter save guard failed");
        }
    }
}
