//! Response caching middleware for the Salvo web framework.
//!
//! This middleware intercepts HTTP responses and caches them for subsequent
//! requests, reducing server load and improving response times for cacheable
//! content.
//!
//! # What Gets Cached
//!
//! The cache stores the complete response including:
//! - HTTP status code
//! - Response headers
//! - Response body (except for streaming responses)
//!
//! # Key Components
//!
//! - [`CacheIssuer`]: Determines the cache key for each request
//! - [`CacheStore`]: Backend storage for cached responses
//! - [`Cache`]: The middleware handler
//!
//! # Default Implementations
//!
//! - [`RequestIssuer`]: Generates cache keys from the request URI and method
//! - [`MokaStore`]: High-performance concurrent cache backed by [`moka`]
//!
//! # Example
//!
//! ```ignore
//! use std::time::Duration;
//! use salvo_cache::{Cache, MokaStore, RequestIssuer};
//! use salvo_core::prelude::*;
//!
//! let cache = Cache::new(
//!     MokaStore::builder()
//!         .time_to_live(Duration::from_secs(300))  // Cache for 5 minutes
//!         .build(),
//!     RequestIssuer::default(),
//! );
//!
//! let router = Router::new()
//!     .hoop(cache)
//!     .get(my_expensive_handler);
//! ```
//!
//! # Custom Cache Keys
//!
//! Implement [`CacheIssuer`] to customize cache key generation:
//!
//! ```ignore
//! use salvo_cache::CacheIssuer;
//!
//! struct UserBasedIssuer;
//! impl CacheIssuer for UserBasedIssuer {
//!     type Key = String;
//!
//!     async fn issue(&self, req: &mut Request, depot: &Depot) -> Option<Self::Key> {
//!         // Cache per user + path
//!         let user_id = depot.get::<String>("user_id").ok()?;
//!         Some(format!("{}:{}", user_id, req.uri().path()))
//!     }
//! }
//! ```
//!
//! # Skipping Cache
//!
//! By default, only GET requests are cached. Use the `skipper` method to customize:
//!
//! ```ignore
//! let cache = Cache::new(store, issuer)
//!     .skipper(|req, _depot| req.uri().path().starts_with("/api/"));
//! ```
//!
//! # Limitations
//!
//! - Streaming responses ([`ResBody::Stream`]) cannot be cached
//! - Error responses are not cached
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::borrow::Borrow;
use std::collections::VecDeque;
use std::error::Error as StdError;
use std::fmt::{self, Debug, Formatter};
use std::hash::Hash;

use bytes::Bytes;
use salvo_core::handler::Skipper;
use salvo_core::http::{HeaderMap, ResBody, StatusCode};
use salvo_core::{Depot, Error, FlowCtrl, Handler, Request, Response, async_trait};

mod skipper;
pub use skipper::MethodSkipper;

#[macro_use]
mod cfg;

cfg_feature! {
    #![feature = "moka-store"]

    pub mod moka_store;
    pub use moka_store::{MokaStore};
}

/// Issuer
pub trait CacheIssuer: Send + Sync + 'static {
    /// The key is used to identify the rate limit.
    type Key: Hash + Eq + Send + Sync + 'static;
    /// Issue a new key for the request. If it returns `None`, the request will not be cached.
    fn issue(
        &self,
        req: &mut Request,
        depot: &Depot,
    ) -> impl Future<Output = Option<Self::Key>> + Send;
}
impl<F, K> CacheIssuer for F
where
    F: Fn(&mut Request, &Depot) -> Option<K> + Send + Sync + 'static,
    K: Hash + Eq + Send + Sync + 'static,
{
    type Key = K;
    async fn issue(&self, req: &mut Request, depot: &Depot) -> Option<Self::Key> {
        (self)(req, depot)
    }
}

/// Identify user by Request Uri.
#[derive(Clone, Debug)]
pub struct RequestIssuer {
    use_scheme: bool,
    use_authority: bool,
    use_path: bool,
    use_query: bool,
    use_method: bool,
}
impl Default for RequestIssuer {
    fn default() -> Self {
        Self::new()
    }
}
impl RequestIssuer {
    /// Create a new `RequestIssuer`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            use_scheme: true,
            use_authority: true,
            use_path: true,
            use_query: true,
            use_method: true,
        }
    }
    /// Whether to use the request's URI scheme when generating the key.
    #[must_use]
    pub fn use_scheme(mut self, value: bool) -> Self {
        self.use_scheme = value;
        self
    }
    /// Whether to use the request's URI authority when generating the key.
    #[must_use]
    pub fn use_authority(mut self, value: bool) -> Self {
        self.use_authority = value;
        self
    }
    /// Whether to use the request's URI path when generating the key.
    #[must_use]
    pub fn use_path(mut self, value: bool) -> Self {
        self.use_path = value;
        self
    }
    /// Whether to use the request's URI query when generating the key.
    #[must_use]
    pub fn use_query(mut self, value: bool) -> Self {
        self.use_query = value;
        self
    }
    /// Whether to use the request method when generating the key.
    #[must_use]
    pub fn use_method(mut self, value: bool) -> Self {
        self.use_method = value;
        self
    }
}

impl CacheIssuer for RequestIssuer {
    type Key = String;
    async fn issue(&self, req: &mut Request, _depot: &Depot) -> Option<Self::Key> {
        let mut key = String::new();
        if self.use_scheme
            && let Some(scheme) = req.uri().scheme_str()
        {
            key.push_str(scheme);
            key.push_str("://");
        }
        if self.use_authority
            && let Some(authority) = req.uri().authority()
        {
            key.push_str(authority.as_str());
        }
        if self.use_path {
            key.push_str(req.uri().path());
        }
        if self.use_query
            && let Some(query) = req.uri().query()
        {
            key.push('?');
            key.push_str(query);
        }
        if self.use_method {
            key.push('|');
            key.push_str(req.method().as_str());
        }
        Some(key)
    }
}

/// Store cache.
pub trait CacheStore: Send + Sync + 'static {
    /// Error type for CacheStore.
    type Error: StdError + Sync + Send + 'static;
    /// Key
    type Key: Hash + Eq + Send + Clone + 'static;
    /// Get the cache item from the store.
    fn load_entry<Q>(&self, key: &Q) -> impl Future<Output = Option<CachedEntry>> + Send
    where
        Self::Key: Borrow<Q>,
        Q: Hash + Eq + Sync;
    /// Save the cache item to the store.
    fn save_entry(
        &self,
        key: Self::Key,
        data: CachedEntry,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// `CachedBody` is used to save the response body to `CacheStore`.
///
/// [`ResBody`] has a Stream type, which is not `Send + Sync`, so we need to convert it to
/// `CachedBody`. If the response's body is [`ResBody::Stream`], it will not be cached.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CachedBody {
    /// No body.
    None,
    /// Single bytes body.
    Once(Bytes),
    /// Chunks body.
    Chunks(VecDeque<Bytes>),
}
impl TryFrom<&ResBody> for CachedBody {
    type Error = Error;
    fn try_from(body: &ResBody) -> Result<Self, Self::Error> {
        match body {
            ResBody::None => Ok(Self::None),
            ResBody::Once(bytes) => Ok(Self::Once(bytes.to_owned())),
            ResBody::Chunks(chunks) => Ok(Self::Chunks(chunks.to_owned())),
            _ => Err(Error::other("unsupported body type")),
        }
    }
}
impl From<CachedBody> for ResBody {
    fn from(body: CachedBody) -> Self {
        match body {
            CachedBody::None => Self::None,
            CachedBody::Once(bytes) => Self::Once(bytes),
            CachedBody::Chunks(chunks) => Self::Chunks(chunks),
        }
    }
}

/// Cached entry which will be stored in the cache store.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct CachedEntry {
    /// Response status.
    pub status: Option<StatusCode>,
    /// Response headers.
    pub headers: HeaderMap,
    /// Response body.
    ///
    /// *Notice: If the response's body is streaming, it will be ignored and not cached.
    pub body: CachedBody,
}
impl CachedEntry {
    /// Create a new `CachedEntry`.
    pub fn new(status: Option<StatusCode>, headers: HeaderMap, body: CachedBody) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    /// Get the response status.
    pub fn status(&self) -> Option<StatusCode> {
        self.status
    }

    /// Get the response headers.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Get the response body.
    ///
    /// *Notice: If the response's body is streaming, it will be ignored and not cached.
    pub fn body(&self) -> &CachedBody {
        &self.body
    }
}

/// Cache middleware.
///
/// # Example
///
/// ```
/// use std::time::Duration;
///
/// use salvo_cache::{Cache, MokaStore, RequestIssuer};
/// use salvo_core::Router;
///
/// let cache = Cache::new(
///     MokaStore::builder()
///         .time_to_live(Duration::from_secs(60))
///         .build(),
///     RequestIssuer::default(),
/// );
/// let router = Router::new().hoop(cache);
/// ```
#[non_exhaustive]
pub struct Cache<S, I> {
    /// Cache store.
    pub store: S,
    /// Cache issuer.
    pub issuer: I,
    /// Skipper.
    pub skipper: Box<dyn Skipper>,
}
impl<S, I> Debug for Cache<S, I>
where
    S: Debug,
    I: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cache")
            .field("store", &self.store)
            .field("issuer", &self.issuer)
            .finish()
    }
}

impl<S, I> Cache<S, I> {
    /// Create a new `Cache`.
    #[inline]
    #[must_use]
    pub fn new(store: S, issuer: I) -> Self {
        let skipper = MethodSkipper::new().skip_all().skip_get(false);
        Self {
            store,
            issuer,
            skipper: Box::new(skipper),
        }
    }
    /// Sets skipper and returns a new `Cache`.
    #[inline]
    #[must_use]
    pub fn skipper(mut self, skipper: impl Skipper) -> Self {
        self.skipper = Box::new(skipper);
        self
    }
}

#[async_trait]
impl<S, I> Handler for Cache<S, I>
where
    S: CacheStore<Key = I::Key>,
    I: CacheIssuer,
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
            return;
        };
        let Some(cache) = self.store.load_entry(&key).await else {
            ctrl.call_next(req, depot, res).await;
            if !res.body.is_stream() && !res.body.is_error() {
                let headers = res.headers().clone();
                let body = TryInto::<CachedBody>::try_into(&res.body);
                match body {
                    Ok(body) => {
                        let cached_data = CachedEntry::new(res.status_code, headers, body);
                        if let Err(e) = self.store.save_entry(key, cached_data).await {
                            tracing::error!(error = ?e, "cache failed");
                        }
                    }
                    Err(e) => tracing::error!(error = ?e, "cache failed"),
                }
            }
            return;
        };
        let CachedEntry {
            status,
            headers,
            body,
        } = cache;
        if let Some(status) = status {
            res.status_code(status);
        }
        *res.headers_mut() = headers;
        *res.body_mut() = body.into();
        ctrl.skip_rest();
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};
    use salvo_core::http::HeaderMap;
    use std::collections::VecDeque;
    use time::OffsetDateTime;

    use super::*;

    #[handler]
    async fn cached() -> String {
        format!(
            "Hello World, my birth time is {}",
            OffsetDateTime::now_utc()
        )
    }

    #[tokio::test]
    async fn test_cache() {
        let cache = Cache::new(
            MokaStore::builder()
                .time_to_live(std::time::Duration::from_secs(5))
                .build(),
            RequestIssuer::default(),
        );
        let router = Router::new().hoop(cache).goal(cached);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let content0 = res.take_string().await.unwrap();

        let mut res = TestClient::get("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let content1 = res.take_string().await.unwrap();
        assert_eq!(content0, content1);

        tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;
        let mut res = TestClient::post("http://127.0.0.1:5801")
            .send(&service)
            .await;
        let content2 = res.take_string().await.unwrap();

        assert_ne!(content0, content2);
    }

    // Tests for RequestIssuer
    #[test]
    fn test_request_issuer_new() {
        let issuer = RequestIssuer::new();
        assert!(issuer.use_scheme);
        assert!(issuer.use_authority);
        assert!(issuer.use_path);
        assert!(issuer.use_query);
        assert!(issuer.use_method);
    }

    #[test]
    fn test_request_issuer_default() {
        let issuer = RequestIssuer::default();
        assert!(issuer.use_scheme);
        assert!(issuer.use_authority);
        assert!(issuer.use_path);
        assert!(issuer.use_query);
        assert!(issuer.use_method);
    }

    #[test]
    fn test_request_issuer_use_scheme() {
        let issuer = RequestIssuer::new().use_scheme(false);
        assert!(!issuer.use_scheme);
        assert!(issuer.use_authority);
    }

    #[test]
    fn test_request_issuer_use_authority() {
        let issuer = RequestIssuer::new().use_authority(false);
        assert!(issuer.use_scheme);
        assert!(!issuer.use_authority);
    }

    #[test]
    fn test_request_issuer_use_path() {
        let issuer = RequestIssuer::new().use_path(false);
        assert!(!issuer.use_path);
    }

    #[test]
    fn test_request_issuer_use_query() {
        let issuer = RequestIssuer::new().use_query(false);
        assert!(!issuer.use_query);
    }

    #[test]
    fn test_request_issuer_use_method() {
        let issuer = RequestIssuer::new().use_method(false);
        assert!(!issuer.use_method);
    }

    #[test]
    fn test_request_issuer_chain() {
        let issuer = RequestIssuer::new()
            .use_scheme(false)
            .use_authority(false)
            .use_path(true)
            .use_query(false)
            .use_method(true);
        assert!(!issuer.use_scheme);
        assert!(!issuer.use_authority);
        assert!(issuer.use_path);
        assert!(!issuer.use_query);
        assert!(issuer.use_method);
    }

    #[test]
    fn test_request_issuer_debug() {
        let issuer = RequestIssuer::new();
        let debug_str = format!("{:?}", issuer);
        assert!(debug_str.contains("RequestIssuer"));
        assert!(debug_str.contains("use_scheme"));
    }

    #[test]
    fn test_request_issuer_clone() {
        let issuer = RequestIssuer::new().use_scheme(false);
        let cloned = issuer.clone();
        assert_eq!(issuer.use_scheme, cloned.use_scheme);
        assert_eq!(issuer.use_authority, cloned.use_authority);
    }

    // Tests for CachedBody
    #[test]
    fn test_cached_body_none() {
        let body = CachedBody::None;
        assert_eq!(body, CachedBody::None);
    }

    #[test]
    fn test_cached_body_once() {
        let bytes = Bytes::from("test data");
        let body = CachedBody::Once(bytes.clone());
        assert_eq!(body, CachedBody::Once(bytes));
    }

    #[test]
    fn test_cached_body_chunks() {
        let mut chunks = VecDeque::new();
        chunks.push_back(Bytes::from("chunk1"));
        chunks.push_back(Bytes::from("chunk2"));
        let body = CachedBody::Chunks(chunks.clone());
        assert_eq!(body, CachedBody::Chunks(chunks));
    }

    #[test]
    fn test_cached_body_try_from_res_body_none() {
        let res_body = ResBody::None;
        let result: Result<CachedBody, _> = (&res_body).try_into();
        assert_eq!(result.unwrap(), CachedBody::None);
    }

    #[test]
    fn test_cached_body_try_from_res_body_once() {
        let bytes = Bytes::from("test");
        let res_body = ResBody::Once(bytes.clone());
        let result: Result<CachedBody, _> = (&res_body).try_into();
        assert_eq!(result.unwrap(), CachedBody::Once(bytes));
    }

    #[test]
    fn test_cached_body_try_from_res_body_chunks() {
        let mut chunks = VecDeque::new();
        chunks.push_back(Bytes::from("chunk1"));
        chunks.push_back(Bytes::from("chunk2"));
        let res_body = ResBody::Chunks(chunks.clone());
        let result: Result<CachedBody, _> = (&res_body).try_into();
        assert_eq!(result.unwrap(), CachedBody::Chunks(chunks));
    }

    #[test]
    fn test_cached_body_into_res_body_none() {
        let cb = CachedBody::None;
        let res_body: ResBody = cb.into();
        assert!(matches!(res_body, ResBody::None));
    }

    #[test]
    fn test_cached_body_into_res_body_once() {
        let bytes = Bytes::from("test");
        let cb = CachedBody::Once(bytes.clone());
        let res_body: ResBody = cb.into();
        assert!(matches!(res_body, ResBody::Once(b) if b == bytes));
    }

    #[test]
    fn test_cached_body_into_res_body_chunks() {
        let mut chunks = VecDeque::new();
        chunks.push_back(Bytes::from("chunk1"));
        let cb = CachedBody::Chunks(chunks);
        let res_body: ResBody = cb.into();
        assert!(matches!(res_body, ResBody::Chunks(_)));
    }

    #[test]
    fn test_cached_body_debug() {
        let body = CachedBody::None;
        let debug_str = format!("{:?}", body);
        assert!(debug_str.contains("None"));

        let body = CachedBody::Once(Bytes::from("test"));
        let debug_str = format!("{:?}", body);
        assert!(debug_str.contains("Once"));
    }

    #[test]
    fn test_cached_body_clone() {
        let body = CachedBody::Once(Bytes::from("test"));
        let cloned = body.clone();
        assert_eq!(body, cloned);
    }

    // Tests for CachedEntry
    #[test]
    fn test_cached_entry_new() {
        let entry = CachedEntry::new(
            Some(StatusCode::OK),
            HeaderMap::new(),
            CachedBody::None,
        );
        assert_eq!(entry.status, Some(StatusCode::OK));
        assert!(entry.headers.is_empty());
        assert_eq!(entry.body, CachedBody::None);
    }

    #[test]
    fn test_cached_entry_status() {
        let entry = CachedEntry::new(
            Some(StatusCode::NOT_FOUND),
            HeaderMap::new(),
            CachedBody::None,
        );
        assert_eq!(entry.status(), Some(StatusCode::NOT_FOUND));
    }

    #[test]
    fn test_cached_entry_status_none() {
        let entry = CachedEntry::new(None, HeaderMap::new(), CachedBody::None);
        assert_eq!(entry.status(), None);
    }

    #[test]
    fn test_cached_entry_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        let entry = CachedEntry::new(Some(StatusCode::OK), headers.clone(), CachedBody::None);
        assert_eq!(entry.headers().len(), 1);
        assert!(entry.headers().contains_key("Content-Type"));
    }

    #[test]
    fn test_cached_entry_body() {
        let body = CachedBody::Once(Bytes::from("test body"));
        let entry = CachedEntry::new(Some(StatusCode::OK), HeaderMap::new(), body.clone());
        assert_eq!(entry.body(), &body);
    }

    #[test]
    fn test_cached_entry_debug() {
        let entry = CachedEntry::new(
            Some(StatusCode::OK),
            HeaderMap::new(),
            CachedBody::None,
        );
        let debug_str = format!("{:?}", entry);
        assert!(debug_str.contains("CachedEntry"));
        assert!(debug_str.contains("status"));
    }

    #[test]
    fn test_cached_entry_clone() {
        let entry = CachedEntry::new(
            Some(StatusCode::OK),
            HeaderMap::new(),
            CachedBody::Once(Bytes::from("test")),
        );
        let cloned = entry.clone();
        assert_eq!(entry.status, cloned.status);
        assert_eq!(entry.body, cloned.body);
    }

    // Tests for Cache
    #[test]
    fn test_cache_new() {
        let cache = Cache::new(
            MokaStore::<String>::new(100),
            RequestIssuer::default(),
        );
        assert!(format!("{:?}", cache).contains("Cache"));
    }

    #[test]
    fn test_cache_debug() {
        let cache = Cache::new(
            MokaStore::<String>::new(100),
            RequestIssuer::default(),
        );
        let debug_str = format!("{:?}", cache);
        assert!(debug_str.contains("Cache"));
        assert!(debug_str.contains("store"));
        assert!(debug_str.contains("issuer"));
    }

    #[tokio::test]
    async fn test_cache_same_path_same_content() {
        let cache = Cache::new(
            MokaStore::builder()
                .time_to_live(std::time::Duration::from_secs(60))
                .build(),
            RequestIssuer::default(),
        );
        let router = Router::new().hoop(cache).goal(cached);
        let service = Service::new(router);

        let mut res1 = TestClient::get("http://127.0.0.1:5801/same-path")
            .send(&service)
            .await;
        let content1 = res1.take_string().await.unwrap();

        let mut res2 = TestClient::get("http://127.0.0.1:5801/same-path")
            .send(&service)
            .await;
        let content2 = res2.take_string().await.unwrap();

        // Same path should return cached content
        assert_eq!(content1, content2);
    }
}
