//! Rate limiting middleware for Salvo.
//!
//! This middleware protects your server from abuse by limiting the number of
//! requests a client can make within a specified time period. It's essential
//! for preventing denial-of-service attacks and ensuring fair resource usage.
//!
//! # Key Components
//!
//! | Component | Purpose |
//! |-----------|---------|
//! | [`RateIssuer`] | Identifies clients (by IP, user ID, API key, etc.) |
//! | [`QuotaGetter`] | Defines rate limits for each client |
//! | [`RateGuard`] | Implements the limiting algorithm |
//! | [`RateStore`] | Stores rate limit state |
//!
//! # Built-in Implementations
//!
//! ## Issuers
//! - [`RemoteIpIssuer`]: Identifies clients by IP address
//!
//! ## Guards (Algorithms)
//! - `FixedGuard`: Fixed window algorithm (requires `fixed-guard` feature)
//! - `SlidingGuard`: Sliding window algorithm (requires `sliding-guard` feature)
//!
//! ## Stores
//! - [`MokaStore`]: In-memory store backed by moka (requires `moka-store` feature)
//!
//! # Example
//!
//! Basic rate limiting by IP address:
//!
//! ```ignore
//! use salvo_rate_limiter::{RateLimiter, RemoteIpIssuer, BasicQuota, FixedGuard, MokaStore};
//! use salvo_core::prelude::*;
//!
//! let limiter = RateLimiter::new(
//!     FixedGuard::default(),
//!     MokaStore::default(),
//!     RemoteIpIssuer,
//!     BasicQuota::per_minute(100),  // 100 requests per minute
//! );
//!
//! let router = Router::new()
//!     .hoop(limiter)
//!     .get(my_handler);
//! ```
//!
//! # Custom Quotas Per User
//!
//! Different users can have different rate limits:
//!
//! ```ignore
//! use salvo_rate_limiter::{QuotaGetter, BasicQuota};
//!
//! struct TieredQuota;
//! impl QuotaGetter<String> for TieredQuota {
//!     type Quota = BasicQuota;
//!     type Error = salvo_core::Error;
//!
//!     async fn get<Q>(&self, user_id: &Q) -> Result<Self::Quota, Self::Error>
//!     where
//!         String: std::borrow::Borrow<Q>,
//!         Q: std::hash::Hash + Eq + Sync,
//!     {
//!         // Premium users get higher limits
//!         if is_premium_user(user_id) {
//!             Ok(BasicQuota::per_minute(1000))
//!         } else {
//!             Ok(BasicQuota::per_minute(60))
//!         }
//!     }
//! }
//! ```
//!
//! # Response Headers
//!
//! Enable rate limit headers in responses with `.add_headers(true)`:
//!
//! - `X-RateLimit-Limit`: Maximum requests allowed
//! - `X-RateLimit-Remaining`: Requests remaining in current window
//! - `X-RateLimit-Reset`: Unix timestamp when the limit resets
//!
//! # HTTP Status
//!
//! When the limit is exceeded, returns `429 Too Many Requests`.
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::borrow::Borrow;
use std::error::Error as StdError;
use std::fmt::{self, Debug, Formatter};
use std::hash::Hash;
use std::net::IpAddr;

use salvo_core::handler::{Skipper, none_skipper};
use salvo_core::http::{HeaderValue, Request, Response, StatusCode, StatusError};
use salvo_core::{Depot, FlowCtrl, Handler, async_trait};

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
pub trait RateIssuer: Send + Sync + 'static {
    /// The key is used to identify the rate limit.
    type Key: Hash + Eq + Send + Sync + 'static;
    /// Issue a new key for the request.
    fn issue(
        &self,
        req: &mut Request,
        depot: &Depot,
    ) -> impl Future<Output = Option<Self::Key>> + Send;
}
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

/// Identify user by the direct connection IP address.
///
/// # Security Note
///
/// This issuer uses `req.remote_addr()` which returns the IP of the direct
/// connection. When your application is behind a reverse proxy or load balancer,
/// this will be the proxy's IP, not the client's real IP.
///
/// For applications behind proxies, use [`RealIpIssuer`] instead, which can
/// extract the client IP from headers like `X-Forwarded-For` or `X-Real-IP`.
///
/// **Warning**: Never use `RealIpIssuer` without a trusted proxy, as clients
/// can forge these headers to bypass rate limiting.
#[derive(Debug, Clone, Copy, Default)]
pub struct RemoteIpIssuer;
impl RateIssuer for RemoteIpIssuer {
    type Key = IpAddr;
    async fn issue(&self, req: &mut Request, _depot: &Depot) -> Option<Self::Key> {
        req.remote_addr().ip()
    }
}

/// Identify user by their real IP address, supporting proxy headers.
///
/// This issuer attempts to extract the client's real IP address by checking
/// headers in the following order:
/// 1. `X-Forwarded-For` (first IP in the list)
/// 2. `X-Real-IP`
/// 3. Falls back to `remote_addr()` if no headers are present
///
/// # Security Warning
///
/// **Only use this issuer when your application is behind a TRUSTED proxy!**
///
/// If clients can connect directly to your application (bypassing the proxy),
/// they can forge these headers to:
/// - Bypass rate limiting by spoofing different IP addresses
/// - Impersonate other users
///
/// Ensure your proxy is configured to:
/// - Overwrite (not append to) the `X-Forwarded-For` header
/// - Block direct connections to your application
///
/// # Example
///
/// ```ignore
/// use salvo_rate_limiter::{RateLimiter, RealIpIssuer, BasicQuota, FixedGuard, MokaStore};
///
/// let limiter = RateLimiter::new(
///     FixedGuard::default(),
///     MokaStore::default(),
///     RealIpIssuer::new(),
///     BasicQuota::per_minute(100),
/// );
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct RealIpIssuer;

impl RealIpIssuer {
    /// Create a new `RealIpIssuer`.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl RateIssuer for RealIpIssuer {
    type Key = IpAddr;

    async fn issue(&self, req: &mut Request, _depot: &Depot) -> Option<Self::Key> {
        // Try X-Forwarded-For header first (common with most reverse proxies)
        if let Some(xff) = req.headers().get("x-forwarded-for") {
            if let Ok(xff_str) = xff.to_str() {
                // X-Forwarded-For can contain multiple IPs: "client, proxy1, proxy2"
                // The first one is the original client IP
                if let Some(first_ip) = xff_str.split(',').next() {
                    if let Ok(ip) = first_ip.trim().parse::<IpAddr>() {
                        return Some(ip);
                    }
                }
            }
        }

        // Try X-Real-IP header (used by nginx)
        if let Some(real_ip) = req.headers().get("x-real-ip") {
            if let Ok(real_ip_str) = real_ip.to_str() {
                if let Ok(ip) = real_ip_str.trim().parse::<IpAddr>() {
                    return Some(ip);
                }
            }
        }

        // Fall back to remote address
        req.remote_addr().ip()
    }
}

/// `RateGuard` is strategy to verify is the request exceeded quota
pub trait RateGuard: Clone + Send + Sync + 'static {
    /// The quota for the rate limit.
    type Quota: Clone + Send + Sync + 'static;
    /// Verify is current request exceed the quota.
    fn verify(&mut self, quota: &Self::Quota) -> impl Future<Output = bool> + Send;

    /// Returns the remaining quota.
    fn remaining(&self, quota: &Self::Quota) -> impl Future<Output = usize> + Send;

    /// Returns the reset time.
    fn reset(&self, quota: &Self::Quota) -> impl Future<Output = i64> + Send;

    /// Returns the limit.
    fn limit(&self, quota: &Self::Quota) -> impl Future<Output = usize> + Send;
}

/// `RateStore` is used to store rate limit data.
pub trait RateStore: Send + Sync + 'static {
    /// Error type for RateStore.
    type Error: StdError;
    /// Key
    type Key: Hash + Eq + Send + Clone + 'static;
    /// Saved guard.
    type Guard;
    /// Get the guard from the store.
    fn load_guard<Q>(
        &self,
        key: &Q,
        refer: &Self::Guard,
    ) -> impl Future<Output = Result<Self::Guard, Self::Error>> + Send
    where
        Self::Key: Borrow<Q>,
        Q: Hash + Eq + Sync;
    /// Save the guard from the store.
    fn save_guard(
        &self,
        key: Self::Key,
        guard: Self::Guard,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// `RateLimiter` is the main struct to used limit user request.
pub struct RateLimiter<G, S, I, Q> {
    guard: G,
    store: S,
    issuer: I,
    quota_getter: Q,
    add_headers: bool,
    skipper: Box<dyn Skipper>,
}
impl<G, S, I, Q> Debug for RateLimiter<G, S, I, Q>
where
    G: Debug,
    S: Debug,
    I: Debug,
    Q: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RateLimiter")
            .field("guard", &self.guard)
            .field("store", &self.store)
            .field("issuer", &self.issuer)
            .field("quota_getter", &self.quota_getter)
            .field("add_headers", &self.add_headers)
            .finish()
    }
}

impl<G: RateGuard, S: RateStore, I: RateIssuer, P: QuotaGetter<I::Key>> RateLimiter<G, S, I, P> {
    /// Create a new `RateLimiter`
    #[inline]
    #[must_use]
    pub fn new(guard: G, store: S, issuer: I, quota_getter: P) -> Self {
        Self {
            guard,
            store,
            issuer,
            quota_getter,
            add_headers: false,
            skipper: Box::new(none_skipper),
        }
    }

    /// Sets skipper and returns new `RateLimiter`.
    #[inline]
    #[must_use]
    pub fn with_skipper(mut self, skipper: impl Skipper) -> Self {
        self.skipper = Box::new(skipper);
        self
    }

    /// Sets `add_headers` and returns new `RateLimiter`.
    /// If `add_headers` is true, the rate limit headers will be added to the response.
    #[inline]
    #[must_use]
    pub fn add_headers(mut self, add_headers: bool) -> Self {
        self.add_headers = add_headers;
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
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        if self.skipper.skipped(req, depot) {
            return;
        }
        let Some(key) = self.issuer.issue(req, depot).await else {
            res.render(StatusError::bad_request().brief("Invalid identifier."));
            ctrl.skip_rest();
            return;
        };
        let quota = match self.quota_getter.get(&key).await {
            Ok(quota) => quota,
            Err(e) => {
                tracing::error!(error = ?e, "RateLimiter error: {}", e);
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                ctrl.skip_rest();
                return;
            }
        };
        let mut guard = match self.store.load_guard(&key, &self.guard).await {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!(error = ?e, "RateLimiter error: {}", e);
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                ctrl.skip_rest();
                return;
            }
        };
        let verified = guard.verify(&quota).await;

        if self.add_headers {
            res.headers_mut().insert(
                "X-RateLimit-Limit",
                HeaderValue::from_str(&guard.limit(&quota).await.to_string())
                    .expect("Invalid header value"),
            );
            res.headers_mut().insert(
                "X-RateLimit-Remaining",
                HeaderValue::from_str(&(guard.remaining(&quota).await).to_string())
                    .expect("Invalid header value"),
            );
            res.headers_mut().insert(
                "X-RateLimit-Reset",
                HeaderValue::from_str(&guard.reset(&quota).await.to_string())
                    .expect("Invalid header value"),
            );
        }
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
    use std::sync::LazyLock;

    use salvo_core::Error;
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use super::*;

    struct UserIssuer;
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
    async fn test_fixed_dynamic_quota() {
        static USER_QUOTAS: LazyLock<HashMap<String, BasicQuota>> = LazyLock::new(|| {
            let mut map = HashMap::new();
            map.insert("user1".into(), BasicQuota::per_second(1));
            map.insert("user2".into(), BasicQuota::set_seconds(1, 5));
            map
        });

        struct CustomQuotaGetter;
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

        let mut response = TestClient::get("http://127.0.0.1:8698/limited?user=user1")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "Limited page");

        let response = TestClient::get("http://127.0.0.1:8698/limited?user=user1")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::TOO_MANY_REQUESTS));

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let mut response = TestClient::get("http://127.0.0.1:8698/limited?user=user1")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "Limited page");

        let mut response = TestClient::get("http://127.0.0.1:8698/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "Limited page");

        let response = TestClient::get("http://127.0.0.1:8698/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::TOO_MANY_REQUESTS));

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let response = TestClient::get("http://127.0.0.1:8698/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::TOO_MANY_REQUESTS));

        tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;

        let mut response = TestClient::get("http://127.0.0.1:8698/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "Limited page");
    }

    #[tokio::test]
    async fn test_sliding_dynamic_quota() {
        static USER_QUOTAS: LazyLock<HashMap<String, CelledQuota>> = LazyLock::new(|| {
            let mut map = HashMap::new();
            map.insert("user1".into(), CelledQuota::per_second(1, 1));
            map.insert("user2".into(), CelledQuota::set_seconds(1, 1, 5));
            map
        });

        struct CustomQuotaGetter;
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

        let mut response = TestClient::get("http://127.0.0.1:8698/limited?user=user1")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "Limited page");

        let response = TestClient::get("http://127.0.0.1:8698/limited?user=user1")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::TOO_MANY_REQUESTS));

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let mut response = TestClient::get("http://127.0.0.1:8698/limited?user=user1")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "Limited page");

        let mut response = TestClient::get("http://127.0.0.1:8698/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "Limited page");

        let response = TestClient::get("http://127.0.0.1:8698/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::TOO_MANY_REQUESTS));

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let response = TestClient::get("http://127.0.0.1:8698/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::TOO_MANY_REQUESTS));

        tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;

        let mut response = TestClient::get("http://127.0.0.1:8698/limited?user=user2")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "Limited page");
    }

    // Tests for RemoteIpIssuer
    #[test]
    fn test_remote_ip_issuer_debug() {
        let issuer = RemoteIpIssuer;
        let debug_str = format!("{:?}", issuer);
        assert!(debug_str.contains("RemoteIpIssuer"));
    }
}
