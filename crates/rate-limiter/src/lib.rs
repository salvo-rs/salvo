//! TBD
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::borrow::Borrow;
use std::convert::Infallible;
use std::error::Error as StdError;
use std::future::Future;
use std::hash::Hash;
use std::marker::PhantomData;
use std::time::Duration;

use salvo_core::addr::SocketAddr;
use salvo_core::handler::Skipper;
use salvo_core::http::{Request, Response, StatusCode, StatusError};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};

#[macro_use]
mod cfg;

cfg_feature! {
    #![feature = "memory-store"]

    mod memory_store;
    pub use memory_store::MemoryStore;
}

cfg_feature! {
    #![feature = "fixed-window"]

    mod fixed_window;
    pub use fixed_window::FixedWindow;
}
cfg_feature! {
    #![feature = "sliding-window"]

    mod sliding_window;
    pub use sliding_window::SlidingWindow;
}
cfg_feature! {
    #![feature = "leaky-bucket"]

    mod leaky_bucket;
    pub use leaky_bucket::LeakyBucket;
}
cfg_feature! {
    #![feature = "token-bucket"]

    mod token_bucket;
    pub use token_bucket::TokenBucket;
}

#[async_trait]
pub trait QuotaProvider<Key>: Send + Sync + 'static {
    type Quota: Clone + Send + Sync + 'static;
    type Error: StdError;

    async fn get<Q>(&self, key: &Q) -> Result<Self::Quota, Self::Error>
    where
        Key: Borrow<Q>,
        Q: Hash + Eq + Sync;
}

#[derive(Clone, Debug)]
pub struct SimpleQuota {
    burst: usize,
    period: Duration,
}
impl SimpleQuota {
    pub const fn new(burst: usize, period: Duration) -> Self {
        Self { burst, period }
    }
    pub const fn per_second(burst: usize) -> Self {
        Self::new(burst, Duration::from_secs(1))
    }
    pub const fn per_minute(burst: usize) -> Self {
        Self::new(burst, Duration::from_secs(60))
    }
    pub const fn per_hour(burst: usize) -> Self {
        Self::new(burst, Duration::from_secs(3600))
    }
}

#[async_trait]
impl<Key, T> QuotaProvider<Key> for T
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

/// Issuer
#[async_trait]
pub trait RateIssuer: Send + Sync + 'static {
    type Key: Hash + Eq + Send + Sync + 'static;
    /// Issue a new key for the request.
    async fn issue(&self, req: &mut Request, depot: &Depot) -> Option<Self::Key>;
}
// #[async_trait]
// impl<F, K> RateIssuer for F
// where
//     F: Fn(&mut Request, &Depot) -> Option<K> + Send + Sync + 'static,
//     K: Hash + Eq + Send + Sync + 'static,
// {
//     type Key = K;
//     async fn issue(&self, req: &mut Request, depot: &Depot) -> Option<Self::Key> {
//         (self)(req, depot)
//     }
// }

pub struct RealIpIssuer;
#[async_trait]
impl RateIssuer for RealIpIssuer {
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

/// RateGuard
#[async_trait]
pub trait RateGuard: Clone + Send + Sync + 'static {
    type Quota: Clone + Send + Sync + 'static;
    async fn verify(&mut self, quota: &Self::Quota) -> bool;
}

/// Store rate.
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
    // Save the guard from the store.
    async fn save_guard(&self, key: Self::Key, guard: Self::Guard) -> Result<(), Self::Error>;
}

/// RateLimiter
pub struct RateLimiter<G, S, I, Q> {
    guard: G,
    store: S,
    issuer: I,
    quota_provider: Q,
    skipper: Option<Box<dyn Skipper>>,
}

impl<G: RateGuard, S: RateStore, I: RateIssuer, P: QuotaProvider<I::Key>> RateLimiter<G, S, I, P> {
    /// Create a new RateLimiter
    #[inline]
    pub fn new(guard: G, store: S, issuer: I, quota_provider: P) -> Self {
        Self {
            guard,
            store,
            quota_provider,
            issuer,
            skipper: None,
        }
    }

    /// Set skipper and returns new `RateLimiter`.
    #[inline]
    pub fn with_skipper(mut self, skipper: impl Skipper) -> Self {
        self.skipper = Some(Box::new(skipper));
        self
    }
}

#[async_trait]
impl<G, S, I, P> Handler for RateLimiter<G, S, I, P>
where
    G: RateGuard<Quota = P::Quota>,
    S: RateStore<Key = I::Key, Guard = G>,
    P: QuotaProvider<I::Key>,
    I: RateIssuer,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if let Some(skipper) = &self.skipper {
            if skipper.skipped(req, depot) {
                return;
            }
        }
        let key = match self.issuer.issue(req, depot).await {
            Some(key) => key,
            None => {
                res.set_status_error(StatusError::bad_request().with_detail("invalid identifier"));
                ctrl.skip_rest();
                return;
            }
        };
        let quota = match self.quota_provider.get(&key).await {
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
