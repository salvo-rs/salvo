#![cfg_attr(test, allow(clippy::unwrap_used))]
//! CORS (Cross-Origin Resource Sharing) protection for Salvo web framework.
//!
//! # Important
//!
//! The CORS handler must be added to [`Service`](salvo_core::Service) via `.hoop()`,
//! **not** to [`Router`](salvo_core::Router). This is because browsers send
//! `OPTIONS` preflight requests that don't match any route, and only
//! `Service`-level middleware can intercept them.
//!
//! ```no_run
//! use salvo_core::http::Method;
//! use salvo_core::prelude::*;
//! use salvo_cors::Cors;
//!
//! #[handler]
//! async fn hello() -> &'static str {
//!     "Hello World"
//! }
//!
//! fn main() {
//!     let cors = Cors::new()
//!         .allow_origin("http://localhost:3000")
//!         .allow_methods([Method::GET, Method::POST, Method::DELETE])
//!         .allow_headers("authorization")
//!         .into_handler();
//!
//!     let router = Router::new().get(hello);
//!     // CORS must be on Service, NOT on Router
//!     let _service = Service::new(router).hoop(cors);
//! }
//! ```
//!
//! [CORS]: https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use bytes::{BufMut, BytesMut};
use salvo_core::http::header::{self, HeaderMap, HeaderName, HeaderValue};
use salvo_core::http::{Method, Request, Response, StatusCode};
use salvo_core::{Depot, FlowCtrl, Handler, async_trait};

mod allow_credentials;
mod allow_headers;
mod allow_methods;
mod allow_origin;
mod allow_private_network;
mod expose_headers;
mod inner;
mod max_age;
mod vary;

pub use self::allow_credentials::AllowCredentials;
pub use self::allow_headers::AllowHeaders;
pub use self::allow_methods::AllowMethods;
pub use self::allow_origin::AllowOrigin;
pub use self::allow_private_network::AllowPrivateNetwork;
pub use self::expose_headers::ExposeHeaders;
pub use self::max_age::MaxAge;
pub use self::vary::Vary;

static WILDCARD: HeaderValue = HeaderValue::from_static("*");

/// Represents a wildcard value (`*`) used with some CORS headers such as
/// [`Cors::allow_methods`].
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct Any;

fn separated_by_commas<I>(iter: I) -> Option<HeaderValue>
where
    I: Iterator<Item = HeaderValue>,
{
    // Materialise the iterator so we can size the output buffer in one
    // shot. The previous implementation grew the `BytesMut` per entry via
    // `reserve(val.len() + 1)`, which produced O(N) reallocations when
    // joining many header values (e.g. a long `allow_headers` list).
    let values: Vec<HeaderValue> = iter.collect();
    let (first, rest) = values.split_first()?;

    let total = first.len()
        + rest
            .iter()
            .map(|v| v.len() + 1) // +1 for the leading comma
            .sum::<usize>();

    let mut result = BytesMut::with_capacity(total);
    result.extend_from_slice(first.as_bytes());
    for val in rest {
        result.put_u8(b',');
        result.extend_from_slice(val.as_bytes());
    }

    HeaderValue::from_maybe_shared(result.freeze()).ok()
}

/// [`Cors`] middleware which adds headers for [CORS][mdn].
///
/// After building, call [`.into_handler()`](Cors::into_handler) and add the
/// resulting handler to [`Service`](salvo_core::Service) via `.hoop()`.
/// Do **not** add it to `Router` — preflight `OPTIONS` requests won't reach
/// router-level middleware.
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS
#[derive(Clone, Debug)]
pub struct Cors {
    allow_credentials: AllowCredentials,
    allow_headers: AllowHeaders,
    allow_methods: AllowMethods,
    allow_origin: AllowOrigin,
    allow_private_network: AllowPrivateNetwork,
    expose_headers: ExposeHeaders,
    max_age: MaxAge,
    vary: Vary,
}
impl Default for Cors {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Cors {
    /// Creates a new `Cors`.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            allow_credentials: Default::default(),
            allow_headers: Default::default(),
            allow_methods: Default::default(),
            allow_origin: Default::default(),
            allow_private_network: Default::default(),
            expose_headers: Default::default(),
            max_age: Default::default(),
            vary: Default::default(),
        }
    }

    /// A permissive configuration:
    ///
    /// - All request headers allowed.
    /// - All methods allowed.
    /// - All origins allowed.
    /// - All headers exposed.
    ///
    /// # Security Warning
    ///
    /// **This configuration allows any website to make requests to your API.**
    /// Only use this for:
    /// - Public APIs that don't require authentication
    /// - Development/testing environments
    ///
    /// For production APIs that require authentication, configure CORS explicitly
    /// with specific allowed origins.
    #[must_use]
    pub fn permissive() -> Self {
        Self::new()
            .allow_headers(Any)
            .allow_methods(Any)
            .allow_origin(Any)
            .expose_headers(Any)
    }

    /// A very permissive configuration:
    ///
    /// - **Credentials allowed.**
    /// - The method received in `Access-Control-Request-Method` is sent back as an allowed method.
    /// - The origin of the preflight request is sent back as an allowed origin.
    /// - The header names received in `Access-Control-Request-Headers` are sent back as allowed
    ///   headers.
    /// - No headers are currently exposed, but this may change in the future.
    ///
    /// # Security Warning
    ///
    /// **⚠️ DANGER: This configuration essentially disables CORS protection!**
    ///
    /// By enabling credentials AND mirroring the request origin, you are allowing
    /// ANY website to:
    /// - Make authenticated requests to your API
    /// - Read response data including sensitive information
    /// - Perform actions on behalf of logged-in users (CSRF attacks)
    ///
    /// **This should NEVER be used in production with authentication.**
    ///
    /// Only use this for:
    /// - Local development where security is not a concern
    /// - Internal tools on trusted networks
    ///
    /// For production, always configure explicit allowed origins:
    /// ```ignore
    /// Cors::new()
    ///     .allow_origin("https://your-frontend.com")
    ///     .allow_credentials(true)
    /// ```
    #[must_use]
    pub fn very_permissive() -> Self {
        tracing::warn!(
            "Using Cors::very_permissive() - this disables CORS security and should not be used in production!"
        );
        Self::new()
            .allow_credentials(true)
            .allow_headers(AllowHeaders::mirror_request())
            .allow_methods(AllowMethods::mirror_request())
            .allow_origin(AllowOrigin::mirror_request())
    }

    /// Sets whether to add the `Access-Control-Allow-Credentials` header.
    #[inline]
    #[must_use]
    pub fn allow_credentials(mut self, allow_credentials: impl Into<AllowCredentials>) -> Self {
        self.allow_credentials = allow_credentials.into();
        self
    }

    /// Adds multiple headers to the list of allowed request headers.
    ///
    /// **Note**: These should match the values the browser sends via
    /// `Access-Control-Request-Headers`, e.g.`content-type`.
    ///
    /// # Panics
    ///
    /// Panics if any of the headers are not a valid `http::header::HeaderName`.
    #[inline]
    #[must_use]
    pub fn allow_headers(mut self, headers: impl Into<AllowHeaders>) -> Self {
        self.allow_headers = headers.into();
        self
    }

    /// Sets the `Access-Control-Max-Age` header.
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use salvo_core::prelude::*;
    /// use salvo_cors::Cors;
    ///
    /// let cors = Cors::new().max_age(30); // 30 seconds
    /// let cors = Cors::new().max_age(Duration::from_secs(30)); // or a Duration
    /// ```
    #[inline]
    #[must_use]
    pub fn max_age(mut self, max_age: impl Into<MaxAge>) -> Self {
        self.max_age = max_age.into();
        self
    }

    /// Adds multiple methods to the existing list of allowed request methods.
    ///
    /// # Panics
    ///
    /// Panics if the provided argument is not a valid `http::Method`.
    #[inline]
    #[must_use]
    pub fn allow_methods<I>(mut self, methods: I) -> Self
    where
        I: Into<AllowMethods>,
    {
        self.allow_methods = methods.into();
        self
    }

    /// Set the value of the [`Access-Control-Allow-Origin`][mdn] header.
    /// ```
    /// use salvo_core::http::HeaderValue;
    /// use salvo_cors::Cors;
    ///
    /// let cors = Cors::new().allow_origin("http://example.com".parse::<HeaderValue>().unwrap());
    /// ```
    ///
    /// Multiple origins can be allowed with
    ///
    /// ```
    /// use salvo_cors::Cors;
    ///
    /// let origins = ["http://example.com", "http://api.example.com"];
    ///
    /// let cors = Cors::new().allow_origin(origins);
    /// ```
    ///
    /// All origins can be allowed with
    ///
    /// ```
    /// use salvo_cors::{Any, Cors};
    ///
    /// let cors = Cors::new().allow_origin(Any);
    /// ```
    ///
    /// You can also use a closure
    ///
    /// ```
    /// use salvo_core::http::HeaderValue;
    /// use salvo_core::{Depot, Request};
    /// use salvo_cors::{AllowOrigin, Cors};
    ///
    /// let cors = Cors::new().allow_origin(AllowOrigin::dynamic(
    ///     |origin: Option<&HeaderValue>, _req: &Request, _depot: &Depot| {
    ///         if origin?.as_bytes().ends_with(b".rust-lang.org") {
    ///             origin.cloned()
    ///         } else {
    ///             None
    ///         }
    ///     },
    /// ));
    /// ```
    ///
    /// You can also use an async closure, make sure all the values are owned
    /// before passing into the future:
    ///
    /// ```
    /// # #[derive(Clone)]
    /// # struct Client;
    /// # fn get_api_client() -> Client {
    /// #     Client
    /// # }
    /// # impl Client {
    /// #     async fn fetch_allowed_origins(&self) -> Vec<HeaderValue> {
    /// #         vec![HeaderValue::from_static("http://example.com")]
    /// #     }
    /// #     async fn fetch_allowed_origins_for_path(&self, _path: String) -> Vec<HeaderValue> {
    /// #         vec![HeaderValue::from_static("http://example.com")]
    /// #     }
    /// # }
    /// use salvo_core::http::header::HeaderValue;
    /// use salvo_core::{Depot, Request};
    /// use salvo_cors::{AllowOrigin, Cors};
    ///
    /// let cors = Cors::new().allow_origin(AllowOrigin::dynamic_async(
    ///     |origin: Option<&HeaderValue>, _req: &Request, _depot: &Depot| {
    ///         let origin = origin.cloned();
    ///         async move {
    ///             let client = get_api_client();
    ///             // fetch list of origins that are allowed
    ///             let origins = client.fetch_allowed_origins().await;
    ///             if origins.contains(origin.as_ref()?) {
    ///                 origin
    ///             } else {
    ///                 None
    ///             }
    ///         }
    ///     },
    /// ));
    /// ```
    ///
    /// **Note** that multiple calls to this method will override any previous
    /// calls.
    ///
    /// **Note** origin must contain http or https protocol name.
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Allow-Origin
    #[inline]
    #[must_use]
    pub fn allow_origin(mut self, origin: impl Into<AllowOrigin>) -> Self {
        self.allow_origin = origin.into();
        self
    }

    /// Set the value of the [`Access-Control-Expose-Headers`][mdn] header.
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Expose-Headers
    #[inline]
    #[must_use]
    pub fn expose_headers(mut self, headers: impl Into<ExposeHeaders>) -> Self {
        self.expose_headers = headers.into();
        self
    }

    /// Set the value of the [`Access-Control-Allow-Private-Network`][wicg] header.
    ///
    /// ```
    /// use salvo_cors::Cors;
    ///
    /// let cors = Cors::new().allow_private_network(true);
    /// ```
    ///
    /// [wicg]: https://wicg.github.io/private-network-access/
    #[must_use]
    pub fn allow_private_network<T>(mut self, allow_private_network: T) -> Self
    where
        T: Into<AllowPrivateNetwork>,
    {
        self.allow_private_network = allow_private_network.into();
        self
    }

    /// Set the value(s) of the [`Vary`][mdn] header.
    ///
    /// In contrast to the other headers, this one has a non-empty default of
    /// [`preflight_request_headers()`].
    ///
    /// You only need to set this is you want to remove some of these defaults,
    /// or if you use a closure for one of the other headers and want to add a
    /// vary header accordingly.
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Vary
    #[must_use]
    pub fn vary<T>(mut self, headers: impl Into<Vary>) -> Self {
        self.vary = headers.into();
        self
    }

    /// Returns a new `CorsHandler` using current cors settings.
    pub fn into_handler(self) -> CorsHandler {
        self.ensure_usable_cors_rules();
        CorsHandler::new(self, CallNext::default())
    }

    fn ensure_usable_cors_rules(&self) {
        if self.allow_credentials.is_true() {
            assert!(
                !self.allow_headers.is_wildcard(),
                "Invalid CORS configuration: Cannot combine `Access-Control-Allow-Credentials: true` \
                 with `Access-Control-Allow-Headers: *`"
            );

            assert!(
                !self.allow_methods.is_wildcard(),
                "Invalid CORS configuration: Cannot combine `Access-Control-Allow-Credentials: true` \
                 with `Access-Control-Allow-Methods: *`"
            );

            assert!(
                !self.allow_origin.is_wildcard(),
                "Invalid CORS configuration: Cannot combine `Access-Control-Allow-Credentials: true` \
                 with `Access-Control-Allow-Origin: *`"
            );

            assert!(
                !self.expose_headers.is_wildcard(),
                "Invalid CORS configuration: Cannot combine `Access-Control-Allow-Credentials: true` \
                 with `Access-Control-Expose-Headers: *`"
            );
        }
    }
}

/// Controls when [`CorsHandler`] runs the rest of the middleware/handler chain
/// relative to writing the CORS headers.
#[non_exhaustive]
#[derive(Default, Clone, Copy, Eq, PartialEq, Debug)]
pub enum CallNext {
    /// Run the remaining handlers **before** [`CorsHandler`] writes the CORS
    /// headers onto the response. This is the default and the right choice for
    /// most setups: downstream handlers see the request unmodified, and CORS
    /// headers are appended last.
    #[default]
    Before,
    /// Run the remaining handlers **after** [`CorsHandler`] has written the CORS
    /// headers. Use this when downstream handlers need to inspect or augment the
    /// CORS headers — note that they may also overwrite them.
    After,
}

/// CorsHandler
#[derive(Clone, Debug)]
pub struct CorsHandler {
    cors: Cors,
    call_next: CallNext,
}
impl CorsHandler {
    /// Create a new `CorsHandler`.
    pub fn new(cors: Cors, call_next: CallNext) -> Self {
        Self { cors, call_next }
    }
}

#[async_trait]
impl Handler for CorsHandler {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        if self.call_next == CallNext::Before {
            ctrl.call_next(req, depot, res).await;
        }

        let origin = req.headers().get(&header::ORIGIN);
        let mut headers = HeaderMap::new();

        // These headers are applied to both preflight and subsequent regular CORS requests:
        // https://fetch.spec.whatwg.org/#http-responses
        headers.extend(self.cors.allow_origin.to_header(origin, req, depot).await);
        headers.extend(
            self.cors
                .allow_credentials
                .to_header(origin, req, depot)
                .await,
        );
        headers.extend(
            self.cors
                .allow_private_network
                .to_header(origin, req, depot)
                .await,
        );

        let mut vary_headers = self.cors.vary.values();
        if let Some(first) = vary_headers.next() {
            let mut header = match headers.entry(header::VARY) {
                header::Entry::Occupied(_) => {
                    unreachable!("no vary header inserted up to this point")
                }
                header::Entry::Vacant(v) => v.insert_entry(first),
            };

            for val in vary_headers {
                header.append(val);
            }
        }

        // Return results immediately upon preflight request
        if req.method() == Method::OPTIONS {
            // These headers are applied only to preflight requests
            headers.extend(self.cors.allow_methods.to_header(origin, req, depot).await);
            headers.extend(self.cors.allow_headers.to_header(origin, req, depot).await);
            headers.extend(self.cors.max_age.to_header(origin, req, depot).await);
            res.status_code = Some(StatusCode::NO_CONTENT);
        } else {
            // This header is applied only to non-preflight requests
            headers.extend(self.cors.expose_headers.to_header(origin, req, depot).await);
        }

        res.headers_mut().extend(headers);

        if self.call_next == CallNext::After {
            ctrl.call_next(req, depot, res).await;
        }

        // Run the wildcard/credentials guard last, after every `call_next` point,
        // so it always sees the truly final response regardless of whether the
        // downstream chain ran before or after this handler. A genuine CORS
        // preflight is an OPTIONS request carrying `Access-Control-Request-
        // Method`; a plain actual request may also use OPTIONS without it.
        let is_preflight = req.method() == Method::OPTIONS
            && req
                .headers()
                .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD);
        enforce_credentials_safety(res, is_preflight);
    }
}

/// Defense in depth: when credentials are allowed, none of the *applicable* CORS
/// response headers may be the wildcard `*`. `ensure_usable_cors_rules` asserts
/// this at build time, but only for statically configured credentials
/// (`AllowCredentials::Yes`); a dynamic credentials policy — or a downstream
/// handler that sets its own CORS headers — can otherwise produce this invalid,
/// unsafe combination at runtime.
///
/// Each header is only checked where it is meaningful, so a non-applicable
/// wildcard (e.g. a global middleware that always emits `Access-Control-Allow-
/// Methods: *`) does not strip credentials from a valid response:
/// - `Access-Control-Allow-Origin` — every response (the security-critical case);
/// - `Access-Control-Allow-Methods` / `-Allow-Headers` — preflight responses only;
/// - `Access-Control-Expose-Headers` — non-preflight (actual) responses only.
fn enforce_credentials_safety(res: &mut Response, is_preflight: bool) {
    let credentials = res
        .headers()
        .get_all(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
        .iter()
        .any(|v| v == "true");
    if !credentials {
        return;
    }

    // CORS list headers (methods/headers/expose) may be comma-separated, so a
    // wildcard can appear as a token within a list such as `*, authorization`.
    // Split on commas and compare per token rather than the whole value.
    let has_wildcard_token = |name: &HeaderName| {
        res.headers().get_all(name).iter().any(|v| {
            v.to_str()
                .is_ok_and(|s| s.split(',').any(|token| token.trim() == "*"))
        })
    };

    let mut candidates = vec![header::ACCESS_CONTROL_ALLOW_ORIGIN];
    if is_preflight {
        candidates.push(header::ACCESS_CONTROL_ALLOW_METHODS);
        candidates.push(header::ACCESS_CONTROL_ALLOW_HEADERS);
    } else {
        candidates.push(header::ACCESS_CONTROL_EXPOSE_HEADERS);
    }

    if let Some(name) = candidates.into_iter().find(|name| has_wildcard_token(name)) {
        tracing::error!(
            "CORS misconfiguration: `Access-Control-Allow-Credentials: true` cannot be combined \
             with `{name}: *`; dropping the credentials header"
        );
        res.headers_mut()
            .remove(header::ACCESS_CONTROL_ALLOW_CREDENTIALS);
    }
}

/// Iterator over the three request headers that may be involved in a CORS preflight request.
///
/// This is the default set of header names returned in the `vary` header
pub fn preflight_request_headers() -> impl Iterator<Item = HeaderName> {
    [
        header::ORIGIN,
        header::ACCESS_CONTROL_REQUEST_METHOD,
        header::ACCESS_CONTROL_REQUEST_HEADERS,
    ]
    .into_iter()
}

#[cfg(test)]
mod tests {
    use salvo_core::http::header::*;
    use salvo_core::prelude::*;
    use salvo_core::test::TestClient;

    use super::*;

    #[tokio::test]
    async fn test_cors_drops_credentials_with_wildcard_origin() {
        // A dynamic credentials policy bypasses the build-time assertions, so the
        // runtime guard must drop `Allow-Credentials: true` when the resolved
        // `Allow-Origin` is `*`.
        let cors_handler = Cors::new()
            .allow_origin(AllowOrigin::any())
            .allow_credentials(AllowCredentials::dynamic(|_, _, _| true))
            .allow_methods(vec![Method::GET, Method::OPTIONS])
            .into_handler();

        #[handler]
        async fn hello() -> &'static str {
            "hello"
        }
        let router = Router::new()
            .hoop(cors_handler)
            .push(Router::with_path("hello").goal(hello));
        let service = Service::new(router);

        let res = TestClient::get("http://127.0.0.1/hello")
            .add_header("Origin", "https://evil.example.com", true)
            .send(&service)
            .await;

        assert_eq!(res.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "*");
        assert!(
            res.headers()
                .get(ACCESS_CONTROL_ALLOW_CREDENTIALS)
                .is_none(),
            "credentials header must be dropped when origin is `*`"
        );
    }

    #[tokio::test]
    async fn test_cors_drops_credentials_when_handler_sets_wildcard_origin() {
        // With the default `CallNext::Before`, the downstream handler runs first
        // and may set its own `Access-Control-Allow-Origin: *`. The guard must
        // inspect the final merged response, not just the headers it built.
        #[handler]
        async fn sets_wildcard(res: &mut Response) {
            res.headers_mut()
                .insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
            res.render("ok");
        }

        let cors_handler = Cors::new()
            .allow_credentials(AllowCredentials::dynamic(|_, _, _| true))
            .into_handler();
        let router = Router::new()
            .hoop(cors_handler)
            .push(Router::with_path("hello").goal(sets_wildcard));
        let service = Service::new(router);

        let res = TestClient::get("http://127.0.0.1/hello")
            .add_header("Origin", "https://app.example.com", true)
            .send(&service)
            .await;

        assert!(
            res.headers()
                .get(ACCESS_CONTROL_ALLOW_CREDENTIALS)
                .is_none(),
            "credentials must be dropped when the handler set a wildcard origin"
        );
    }

    #[tokio::test]
    async fn test_cors_drops_credentials_in_after_mode_with_handler_wildcard() {
        // In `CallNext::After` mode the downstream chain runs after this handler,
        // so a handler that sets a wildcard CORS header must still be caught by
        // the guard, which runs after every `call_next` point.
        #[handler]
        async fn sets_wildcard(res: &mut Response) {
            res.headers_mut()
                .insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
            res.render("ok");
        }

        let cors = Cors::new().allow_credentials(AllowCredentials::dynamic(|_, _, _| true));
        let cors_handler = CorsHandler::new(cors, CallNext::After);
        let router = Router::new()
            .hoop(cors_handler)
            .push(Router::with_path("hello").goal(sets_wildcard));
        let service = Service::new(router);

        let res = TestClient::get("http://127.0.0.1/hello")
            .add_header("Origin", "https://app.example.com", true)
            .send(&service)
            .await;

        assert!(
            res.headers()
                .get(ACCESS_CONTROL_ALLOW_CREDENTIALS)
                .is_none(),
            "credentials must be dropped even in After mode when the handler sets a wildcard"
        );
    }

    #[tokio::test]
    async fn test_cors_drops_credentials_with_wildcard_token_in_list() {
        // A wildcard can appear as a token inside a comma-separated list header
        // (e.g. `Access-Control-Allow-Headers: *, authorization`); it must still
        // be detected. `Allow-Headers` is preflight-only, so use an OPTIONS
        // request.
        // A handler sets a comma-separated `Allow-Headers` containing a `*` token.
        #[handler]
        async fn sets_list_wildcard(res: &mut Response) {
            res.headers_mut().insert(
                ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("*, authorization"),
            );
        }
        let cors_handler = Cors::new()
            .allow_origin("https://app.example.com")
            .allow_credentials(AllowCredentials::dynamic(|_, _, _| true))
            .into_handler();
        let router = Router::new()
            .hoop(cors_handler)
            .push(Router::with_path("hello").goal(sets_list_wildcard));
        let service = Service::new(router);

        let res = TestClient::options("http://127.0.0.1/hello")
            .add_header("Origin", "https://app.example.com", true)
            .add_header(ACCESS_CONTROL_REQUEST_METHOD, "GET", true)
            .send(&service)
            .await;

        assert!(
            res.headers()
                .get(ACCESS_CONTROL_ALLOW_CREDENTIALS)
                .is_none(),
            "credentials must be dropped when a list header contains a `*` token"
        );
    }

    #[tokio::test]
    async fn test_cors_keeps_credentials_on_options_without_request_method() {
        // An OPTIONS request without `Access-Control-Request-Method` is an actual
        // request, not a preflight, so a wildcard `Allow-Methods` must not strip
        // credentials.
        #[handler]
        async fn sets_wildcard_methods(res: &mut Response) {
            res.headers_mut()
                .insert(ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("*"));
        }

        let cors_handler = Cors::new()
            .allow_origin("https://app.example.com")
            .allow_credentials(AllowCredentials::dynamic(|_, _, _| true))
            .into_handler();
        let router = Router::new()
            .hoop(cors_handler)
            .push(Router::with_path("hello").goal(sets_wildcard_methods));
        let service = Service::new(router);

        let res = TestClient::options("http://127.0.0.1/hello")
            .add_header("Origin", "https://app.example.com", true)
            .send(&service)
            .await;

        assert_eq!(
            res.headers().get(ACCESS_CONTROL_ALLOW_CREDENTIALS).unwrap(),
            "true",
            "credentials must be kept for a non-preflight OPTIONS (no Request-Method)"
        );
    }

    #[tokio::test]
    async fn test_cors_keeps_credentials_on_actual_request_with_wildcard_methods() {
        // `Allow-Methods` is only meaningful on a preflight response. A wildcard
        // value on a normal (non-OPTIONS) response is ignored by the browser, so
        // the guard must NOT strip credentials in that case (the actual
        // credentialed CORS check uses the specific origin + credentials).
        #[handler]
        async fn sets_wildcard_methods(res: &mut Response) {
            res.headers_mut()
                .insert(ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("*"));
            res.render("ok");
        }

        let cors_handler = Cors::new()
            .allow_origin("https://app.example.com")
            .allow_credentials(AllowCredentials::dynamic(|_, _, _| true))
            .into_handler();
        let router = Router::new()
            .hoop(cors_handler)
            .push(Router::with_path("hello").goal(sets_wildcard_methods));
        let service = Service::new(router);

        let res = TestClient::get("http://127.0.0.1/hello")
            .add_header("Origin", "https://app.example.com", true)
            .send(&service)
            .await;

        assert_eq!(
            res.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
            "https://app.example.com"
        );
        assert_eq!(
            res.headers().get(ACCESS_CONTROL_ALLOW_CREDENTIALS).unwrap(),
            "true",
            "credentials must be kept: wildcard methods is preflight-only and ignored here"
        );
    }

    #[tokio::test]
    async fn test_cors_drops_credentials_with_wildcard_methods() {
        // Origin is an exact (reflected) value, but `Allow-Methods` is `*`;
        // credentials must still be dropped because `*` and credentials are
        // incompatible for any of the CORS response headers.
        let cors_handler = Cors::new()
            .allow_origin("https://app.example.com")
            .allow_methods(Any)
            .allow_credentials(AllowCredentials::dynamic(|_, _, _| true))
            .into_handler();

        #[handler]
        async fn hello() -> &'static str {
            "hello"
        }
        let router = Router::new()
            .hoop(cors_handler)
            .push(Router::with_path("hello").goal(hello));
        let service = Service::new(router);

        let res = TestClient::options("http://127.0.0.1/hello")
            .add_header("Origin", "https://app.example.com", true)
            .add_header(ACCESS_CONTROL_REQUEST_METHOD, "GET", true)
            .send(&service)
            .await;

        assert_eq!(
            res.headers().get(ACCESS_CONTROL_ALLOW_METHODS).unwrap(),
            "*"
        );
        assert_eq!(
            res.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
            "https://app.example.com"
        );
        assert!(
            res.headers()
                .get(ACCESS_CONTROL_ALLOW_CREDENTIALS)
                .is_none(),
            "credentials must be dropped when any CORS header is `*`"
        );
    }

    #[tokio::test]
    async fn test_cors() {
        let cors_handler = Cors::new()
            .allow_origin("https://salvo.rs")
            .allow_methods(vec![Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers(vec![
                "CONTENT-TYPE",
                "Access-Control-Request-Method",
                "Access-Control-Allow-Origin",
                "Access-Control-Allow-Headers",
                "Access-Control-Max-Age",
            ])
            .into_handler();

        #[handler]
        async fn hello() -> &'static str {
            "hello"
        }

        let router = Router::new()
            .hoop(cors_handler)
            .push(Router::with_path("hello").goal(hello));
        let service = Service::new(router);

        async fn options_access(service: &Service, origin: &str) -> Response {
            TestClient::options("http://127.0.0.1:5801/hello")
                .add_header("Origin", origin, true)
                .add_header("Access-Control-Request-Method", "POST", true)
                .add_header("Access-Control-Request-Headers", "Content-Type", true)
                .send(service)
                .await
        }

        let res = TestClient::options("https://salvo.rs").send(&service).await;
        assert!(res.headers().get(ACCESS_CONTROL_ALLOW_METHODS).is_none());

        let res = options_access(&service, "https://salvo.rs").await;
        let headers = res.headers();
        assert!(headers.get(ACCESS_CONTROL_ALLOW_METHODS).is_some());
        assert!(headers.get(ACCESS_CONTROL_ALLOW_HEADERS).is_some());

        let res = TestClient::options("https://google.com")
            .send(&service)
            .await;
        let headers = res.headers();
        assert!(
            headers.get(ACCESS_CONTROL_ALLOW_METHODS).is_none(),
            "POST, GET, DELETE, OPTIONS"
        );
        assert!(headers.get(ACCESS_CONTROL_ALLOW_HEADERS).is_none());
    }

    #[test]
    fn test_separated_by_commas_empty_iter_returns_none() {
        let result = super::separated_by_commas(std::iter::empty::<HeaderValue>());
        assert!(result.is_none());
    }

    #[test]
    fn test_separated_by_commas_joins_values() {
        let values = vec![
            HeaderValue::from_static("content-type"),
            HeaderValue::from_static("authorization"),
            HeaderValue::from_static("x-requested-with"),
        ];

        let joined = super::separated_by_commas(values.into_iter()).expect("non-empty");
        assert_eq!(joined, "content-type,authorization,x-requested-with");
    }
}
