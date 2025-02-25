//! Cache middleware for the Salvo web framework.
//!
//! Cache middleware for Salvo designed to intercept responses and cache them.
//! This middleware will cache the response's StatusCode, Headers, and Body.
//!
//! You can define your custom [`CacheIssuer`] to determine which responses should be cached,
//! or you can use the default [`RequestIssuer`].
//!
//! The default cache store is [`MokaStore`], which is a wrapper of [`moka`].
//! You can define your own cache store by implementing [`CacheStore`].
//!
//! Example: [cache-simple](https://github.com/salvo-rs/salvo/tree/main/examples/cache-simple)
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::borrow::Borrow;
use std::collections::VecDeque;
use std::error::Error as StdError;
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
    pub fn use_scheme(mut self, value: bool) -> Self {
        self.use_scheme = value;
        self
    }
    /// Whether to use the request's URI authority when generating the key.
    pub fn use_authority(mut self, value: bool) -> Self {
        self.use_authority = value;
        self
    }
    /// Whether to use the request's URI path when generating the key.
    pub fn use_path(mut self, value: bool) -> Self {
        self.use_path = value;
        self
    }
    /// Whether to use the request's URI query when generating the key.
    pub fn use_query(mut self, value: bool) -> Self {
        self.use_query = value;
        self
    }
    /// Whether to use the request method when generating the key.
    pub fn use_method(mut self, value: bool) -> Self {
        self.use_method = value;
        self
    }
}

impl CacheIssuer for RequestIssuer {
    type Key = String;
    async fn issue(&self, req: &mut Request, _depot: &Depot) -> Option<Self::Key> {
        let mut key = String::new();
        if self.use_scheme {
            if let Some(scheme) = req.uri().scheme_str() {
                key.push_str(scheme);
                key.push_str("://");
            }
        }
        if self.use_authority {
            if let Some(authority) = req.uri().authority() {
                key.push_str(authority.as_str());
            }
        }
        if self.use_path {
            key.push_str(req.uri().path());
        }
        if self.use_query {
            if let Some(query) = req.uri().query() {
                key.push('?');
                key.push_str(query);
            }
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
/// [`ResBody`] has a Stream type, which is not `Send + Sync`, so we need to convert it to `CachedBody`.
/// If the response's body is [`ResBody::Stream`], it will not be cached.
#[derive(Clone, Debug)]
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
/// use salvo_core::Router;
/// use salvo_cache::{Cache, MokaStore, RequestIssuer};
///
/// let cache = Cache::new(
///     MokaStore::builder().time_to_live(Duration::from_secs(60)).build(),
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

impl<S, I> Cache<S, I> {
    /// Create a new `Cache`.
    #[inline]
    pub fn new(store: S, issuer: I) -> Self {
        let skipper = MethodSkipper::new().skip_all().skip_get(false);
        Cache {
            store,
            issuer,
            skipper: Box::new(skipper),
        }
    }
    /// Sets skipper and returns a new `Cache`.
    #[inline]
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
        let key = match self.issuer.issue(req, depot).await {
            Some(key) => key,
            None => {
                return;
            }
        };
        let cache = match self.store.load_entry(&key).await {
            Some(cache) => cache,
            None => {
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
            }
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
    use super::*;
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};
    use time::OffsetDateTime;

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
}
