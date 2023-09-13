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
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::future_not_send)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::borrow::Borrow;
use std::error::Error as StdError;
use std::hash::Hash;

use salvo_core::conn::SocketAddr;
use salvo_core::handler::{none_skipper, Skipper};
use salvo_core::http::{Request, Response, StatusCode, StatusError};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};

mod quota;
pub use quota::{BasicQuota, CelledQuota, QuotaGetter};
#[macro_use]
mod cfg;

cfg_feature! {
    #![feature = "moka-store"]

    mod moka_store;
    pub use moka_store::MokaStore;
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
            SocketAddr::IPv4(addr) => Some(addr.ip().to_string()),
            SocketAddr::IPv6(addr) => Some(addr.ip().to_string()),
            _ => None,
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
            skipper: Box::new(none_skipper),
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
                res.render(StatusError::bad_request().brief("Invalid identifier."));
                ctrl.skip_rest();
                return;
            }
        };
        let quota = match self.quota_getter.get(&key).await {
            Ok(quota) => quota,
            Err(e) => {
                tracing::error!(error = ?e, "RateLimiter error");
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                ctrl.skip_rest();
                return;
            }
        };
        let mut guard = match self.store.load_guard(&key, &self.guard).await {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!(error = ?e, "RateLimiter error");
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                ctrl.skip_rest();
                return;
            }
        };
        let verified = guard.verify(&quota).await;
        if !verified {
            res.status_code(StatusCode::TOO_MANY_REQUESTS);
            ctrl.skip_rest();
        }
        if let Err(e) = self.store.save_guard(key, guard).await {
            tracing::error!(error = ?e, "RateLimiter save guard failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use once_cell::sync::Lazy;
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};
    use salvo_core::Error;

    use super::*;

    struct UserIssuer;
    #[async_trait]
    impl RateIssuer for UserIssuer {
        type Key = String;
        async fn issue(&self, req: &mut Request, _depot: &Depot) -> Option<Self::Key> {
            req.query::<Self::Key>("user")
        }
    }

    #[handler]
    async fn limited() -> &'static str {
        "Limited page"
    }

    #[tokio::test]
    async fn test_fixed_dynmaic_quota() {
        static USER_QUOTAS: Lazy<HashMap<String, BasicQuota>> = Lazy::new(|| {
            let mut map = HashMap::new();
            map.insert("user1".into(), BasicQuota::per_second(1));
            map.insert("user2".into(), BasicQuota::set_seconds(1, 5));
            map
        });

        struct CustomQuotaGetter;
        #[async_trait]
        impl QuotaGetter<String> for CustomQuotaGetter {
            type Quota = BasicQuota;
            type Error = Error;

            async fn get<Q>(&self, key: &Q) -> Result<Self::Quota, Self::Error>
            where
                String: Borrow<Q>,
                Q: Hash + Eq + Sync,
            {
                USER_QUOTAS
                    .get(key)
                    .cloned()
                    .ok_or_else(|| Error::other("user not found"))
            }
        }
        let limiter = RateLimiter::new(
            FixedGuard::default(),
            MokaStore::default(),
            UserIssuer,
            CustomQuotaGetter,
        );
        let router = Router::new().push(Router::with_path("limited").hoop(limiter).get(limited));
        let service = Service::new(router);

        let mut respone = TestClient::get("http://127.0.0.1:5800/limited?user=user1")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::OK));
        assert_eq!(respone.take_string().await.unwrap(), "Limited page");

        let respone = TestClient::get("http://127.0.0.1:5800/limited?user=user1")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::TOO_MANY_REQUESTS));

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let mut respone = TestClient::get("http://127.0.0.1:5800/limited?user=user1")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::OK));
        assert_eq!(respone.take_string().await.unwrap(), "Limited page");

        let mut respone = TestClient::get("http://127.0.0.1:5800/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::OK));
        assert_eq!(respone.take_string().await.unwrap(), "Limited page");

        let respone = TestClient::get("http://127.0.0.1:5800/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::TOO_MANY_REQUESTS));

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let respone = TestClient::get("http://127.0.0.1:5800/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::TOO_MANY_REQUESTS));

        tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;

        let mut respone = TestClient::get("http://127.0.0.1:5800/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::OK));
        assert_eq!(respone.take_string().await.unwrap(), "Limited page");
    }

    #[tokio::test]
    async fn test_sliding_dynmaic_quota() {
        static USER_QUOTAS: Lazy<HashMap<String, CelledQuota>> = Lazy::new(|| {
            let mut map = HashMap::new();
            map.insert("user1".into(), CelledQuota::per_second(1, 1));
            map.insert("user2".into(), CelledQuota::set_seconds(1, 1, 5));
            map
        });

        struct CustomQuotaGetter;
        #[async_trait]
        impl QuotaGetter<String> for CustomQuotaGetter {
            type Quota = CelledQuota;
            type Error = Error;

            async fn get<Q>(&self, key: &Q) -> Result<Self::Quota, Self::Error>
            where
                String: Borrow<Q>,
                Q: Hash + Eq + Sync,
            {
                USER_QUOTAS
                    .get(key)
                    .cloned()
                    .ok_or_else(|| Error::other("user not found"))
            }
        }
        let limiter = RateLimiter::new(
            SlidingGuard::default(),
            MokaStore::default(),
            UserIssuer,
            CustomQuotaGetter,
        );
        let router = Router::new().push(Router::with_path("limited").hoop(limiter).get(limited));
        let service = Service::new(router);

        let mut respone = TestClient::get("http://127.0.0.1:5800/limited?user=user1")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::OK));
        assert_eq!(respone.take_string().await.unwrap(), "Limited page");

        let respone = TestClient::get("http://127.0.0.1:5800/limited?user=user1")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::TOO_MANY_REQUESTS));

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let mut respone = TestClient::get("http://127.0.0.1:5800/limited?user=user1")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::OK));
        assert_eq!(respone.take_string().await.unwrap(), "Limited page");

        let mut respone = TestClient::get("http://127.0.0.1:5800/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::OK));
        assert_eq!(respone.take_string().await.unwrap(), "Limited page");

        let respone = TestClient::get("http://127.0.0.1:5800/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::TOO_MANY_REQUESTS));

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let respone = TestClient::get("http://127.0.0.1:5800/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::TOO_MANY_REQUESTS));

        tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;

        let mut respone = TestClient::get("http://127.0.0.1:5800/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::OK));
        assert_eq!(respone.take_string().await.unwrap(), "Limited page");
    }
}
