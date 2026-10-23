use std::fmt::{self, Debug, Formatter};
use std::future::pending;
use std::pin::Pin;
use std::sync::{Arc, LazyLock};

use headers::{Allow, HeaderMapExt, HeaderValue};
use http::header::{ALT_SVC, CONTENT_TYPE};
use http::uri::Scheme;
use hyper::service::Service as HyperService;
use hyper::{Method, Request as HyperRequest, Response as HyperResponse};

use crate::catcher::{Catcher, write_error_default};
use crate::conn::SocketAddr;
use crate::fuse::FuseConfig;
use crate::handler::{Handler, WhenHoop};
use crate::http::body::{ReqBody, ResBody};
use crate::http::{Mime, Request, Response, StatusCode, StatusError};
use crate::routing::{FlowCtrl, PathState, Router};
use crate::{ConnCtrl, Depot, async_trait};

/// Service http request.
///
/// Automatic 405 responses include methods discoverable through built-in
/// filters and their `and`/`or` combinations. Function filters, callback
/// combinators and custom character checkers are opaque and are not replayed
/// to discover methods. When no method can be inferred, failed routing returns
/// 404. Applications using opaque filters can set their own 405 status and
/// `Allow` header through service middleware.
#[non_exhaustive]
pub struct Service {
    /// The router of this service.
    pub router: Arc<Router>,
    /// The catcher of this service.
    pub catcher: Option<Arc<Catcher>>,
    /// These hoops will always be called when request received.
    pub hoops: Vec<Arc<dyn Handler>>,
    /// The allowed media types of this service.
    pub allowed_media_types: Arc<Vec<Mime>>,
}

impl Debug for Service {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Service")
            .field("router", &self.router)
            .field("catcher", &self.catcher)
            .field("hoops", &self.hoops.len())
            .field("allowed_media_types", &self.allowed_media_types.len())
            .finish()
    }
}

impl Service {
    /// Create a new Service with a [`Router`].
    #[inline]
    pub fn new<T>(router: T) -> Self
    where
        T: Into<Arc<Router>>,
    {
        Self {
            router: router.into(),
            catcher: None,
            hoops: vec![],
            allowed_media_types: Arc::new(vec![]),
        }
    }

    /// Get router in this `Service`.
    #[inline]
    #[must_use]
    pub fn router(&self) -> Arc<Router> {
        self.router.clone()
    }

    /// When the response code is 400-599 and the body is empty, capture and set the error page
    /// content. If no catcher is set, the default error page will be used.
    ///
    /// # Example
    ///
    /// ```
    /// use salvo_core::catcher::Catcher;
    /// use salvo_core::prelude::*;
    ///
    /// #[handler]
    /// async fn handle404(
    ///     &self,
    ///     _req: &Request,
    ///     _depot: &Depot,
    ///     res: &mut Response,
    ///     ctrl: &mut FlowCtrl,
    /// ) {
    ///     if let Some(StatusCode::NOT_FOUND) = res.status_code {
    ///         res.render("Custom 404 Error Page");
    ///         ctrl.skip_rest();
    ///     }
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     Service::new(Router::new()).catcher(Catcher::default().hoop(handle404));
    /// }
    /// ```
    #[inline]
    #[must_use]
    pub fn catcher(mut self, catcher: impl Into<Arc<Catcher>>) -> Self {
        self.catcher = Some(catcher.into());
        self
    }

    /// Add a handler as middleware, it will run the handler when request received.
    #[inline]
    #[must_use]
    pub fn hoop<H: Handler>(mut self, hoop: H) -> Self {
        self.hoops.push(Arc::new(hoop));
        self
    }

    /// Add a handler as middleware. It runs the handler when a request is received,
    /// but only when the filter returns `true`.
    #[inline]
    #[must_use]
    pub fn hoop_when<H, F>(mut self, hoop: H, filter: F) -> Self
    where
        H: Handler,
        F: Fn(&Request, &Depot) -> bool + Send + Sync + 'static,
    {
        self.hoops.push(Arc::new(WhenHoop {
            inner: hoop,
            filter,
        }));
        self
    }

    /// Sets allowed media types list and returns `Self` for write code chained.
    ///
    /// A response with a disallowed Content-Type is replaced with a 415 error
    /// and processed by the configured catcher. HEAD responses remain bodyless.
    ///
    /// # Example
    ///
    /// ```
    /// # use salvo_core::prelude::*;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let service = Service::new(Router::new()).allowed_media_types(vec![mime::TEXT_PLAIN]);
    /// # }
    /// ```
    #[inline]
    #[must_use]
    pub fn allowed_media_types<T>(mut self, allowed_media_types: T) -> Self
    where
        T: Into<Arc<Vec<Mime>>>,
    {
        self.allowed_media_types = allowed_media_types.into();
        self
    }

    #[doc(hidden)]
    #[inline]
    #[must_use]
    pub fn hyper_handler(
        &self,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        http_scheme: Scheme,
        fuse_config: Option<FuseConfig>,
        conn_ctrl: ConnCtrl,
        alt_svc_h3: Option<HeaderValue>,
    ) -> HyperHandler {
        HyperHandler {
            local_addr,
            remote_addr,
            http_scheme,
            state: Arc::new(HyperHandlerState {
                router: self.router.clone(),
                catcher: self.catcher.clone(),
                hoops: self.hoops.clone(),
                allowed_media_types: self.allowed_media_types.clone(),
            }),
            fuse_config,
            conn_ctrl,
            alt_svc_h3,
        }
    }
    /// Handle new request, this function only used for test.
    #[cfg(feature = "test")]
    #[inline]
    pub async fn handle(&self, request: impl Into<Request> + Send) -> Response {
        let request = request.into();
        self.hyper_handler(
            request.local_addr.clone(),
            request.remote_addr.clone(),
            request.scheme.clone(),
            None,
            ConnCtrl::new(),
            None,
        )
        .handle(request)
        .await
    }
}

impl<T> From<T> for Service
where
    T: Into<Arc<Router>>,
{
    #[inline]
    fn from(router: T) -> Self {
        Self::new(router)
    }
}

struct DefaultStatusOK;
#[async_trait]
impl Handler for DefaultStatusOK {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        ctrl.call_next(req, depot, res).await;
        if res.status_code.is_none() {
            res.status_code = Some(StatusCode::OK);
        }
    }
}

/// Cached DefaultStatusOK handler to avoid Arc allocation per request.
static DEFAULT_STATUS_OK_HANDLER: LazyLock<Arc<dyn Handler>> =
    LazyLock::new(|| Arc::new(DefaultStatusOK));

#[doc(hidden)]
pub(crate) struct HyperHandlerState {
    pub(crate) router: Arc<Router>,
    pub(crate) catcher: Option<Arc<Catcher>>,
    pub(crate) hoops: Vec<Arc<dyn Handler>>,
    pub(crate) allowed_media_types: Arc<Vec<Mime>>,
}

#[doc(hidden)]
#[derive(Clone)]
pub struct HyperHandler {
    pub(crate) local_addr: SocketAddr,
    pub(crate) remote_addr: SocketAddr,
    pub(crate) http_scheme: Scheme,
    pub(crate) state: Arc<HyperHandlerState>,
    pub(crate) fuse_config: Option<FuseConfig>,
    pub(crate) conn_ctrl: ConnCtrl,
    pub(crate) alt_svc_h3: Option<HeaderValue>,
}
impl Debug for HyperHandler {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("HyperHandler")
            .field("local_addr", &self.local_addr)
            .field("remote_addr", &self.remote_addr)
            .field("http_scheme", &self.http_scheme)
            .field("router", &self.state.router)
            .field("catcher", &self.state.catcher)
            .field("allowed_media_types", &self.state.allowed_media_types)
            .field("alt_svc_h3", &self.alt_svc_h3)
            .finish()
    }
}
impl HyperHandler {
    /// Handle [`Request`] and returns [`Response`].
    pub fn handle(&self, mut req: Request) -> impl Future<Output = Response> + 'static {
        let state = self.state.clone();
        let conn_ctrl = self.conn_ctrl.clone();
        req.local_addr = self.local_addr.clone();
        req.remote_addr = self.remote_addr.clone();
        // Expose the connection control on the request so protocol-upgrade handlers
        // (e.g. WebSocket) can relax the transport fuse timers for the long-lived
        // connection they are about to take over.
        req.extensions_mut().insert(conn_ctrl.clone());
        #[cfg(feature = "response-observers")]
        let response_observers = Arc::new(crate::http::request::ResponseObservers::default());
        #[cfg(feature = "response-observers")]
        {
            req.response_observers = Arc::downgrade(&response_observers);
        }
        #[cfg(not(feature = "cookie"))]
        let mut res = Response::new();
        #[cfg(feature = "cookie")]
        let mut res = Response::with_request_cookies(&req.cookies);
        if let Some(alt_svc_h3) = &self.alt_svc_h3
            && !res.headers().contains_key(ALT_SVC)
        {
            res.headers_mut().insert(ALT_SVC, alt_svc_h3.clone());
        }
        let mut depot = Depot::new();

        async move {
            let path = req.uri().path().to_owned();
            let mut path_state = PathState::from_borrowed_path(&path);
            let matched = state.router.detect(&mut req, &mut path_state).await;
            let allowed = if matched.is_none() {
                state.router.allowed_methods(&mut req, &path).await
            } else {
                Vec::new()
            };
            let method_not_allowed = !allowed.is_empty();
            if let Some(dm) = matched {
                path_state.params.seal();
                req.params = path_state.params;
                #[cfg(feature = "matched-path")]
                {
                    req.matched_path = path_state.matched_parts.join("/");
                }
                // Set default status code before service hoops executed.
                // We hope all hoops in service can get the correct status code.
                let mut handlers = Vec::with_capacity(state.hoops.len() + dm.hoops.len() + 2);
                handlers.extend(state.hoops.iter().cloned());
                handlers.extend(dm.hoops);
                handlers.push(DEFAULT_STATUS_OK_HANDLER.clone());
                handlers.push(dm.goal);
                let mut ctrl = FlowCtrl::with_conn(handlers, conn_ctrl.clone());
                ctrl.call_next(&mut req, &mut depot, &mut res).await;
                // Set it to default status code again if any hoop set status code to None.
                if res.status_code.is_none() {
                    res.status_code = Some(StatusCode::OK);
                }
            } else if !state.hoops.is_empty() {
                path_state.params.seal();
                req.params = path_state.params;
                // Set default status code before service hoops executed.
                // We hope all hoops in service can get the correct status code.
                if method_not_allowed {
                    res.status_code = Some(StatusCode::METHOD_NOT_ALLOWED);
                } else {
                    res.status_code = Some(StatusCode::NOT_FOUND);
                }
                let mut ctrl = FlowCtrl::with_conn(state.hoops.clone(), conn_ctrl.clone());
                ctrl.call_next(&mut req, &mut depot, &mut res).await;
                // Set it to default status code again if any hoop set status code to None.
                if res.status_code.is_none() && method_not_allowed {
                    res.status_code = Some(StatusCode::METHOD_NOT_ALLOWED);
                }
            } else if method_not_allowed {
                res.status_code = Some(StatusCode::METHOD_NOT_ALLOWED);
            }

            if !state.allowed_media_types.is_empty()
                && let Some(ctype) = res
                    .headers()
                    .get(CONTENT_TYPE)
                    .and_then(|c| c.to_str().ok())
                    .and_then(|c| c.parse::<Mime>().ok())
            {
                let mut is_allowed = false;
                for mime in &*state.allowed_media_types {
                    if mime.type_() == ctype.type_() && mime.subtype() == ctype.subtype() {
                        is_allowed = true;
                        break;
                    }
                }
                if !is_allowed {
                    // Discard the rejected representation and let the catcher
                    // produce the error response, even when the old body was nonempty.
                    res.render(StatusError::unsupported_media_type());
                }
            }

            let status_code = *res.status_code.get_or_insert(StatusCode::NOT_FOUND);
            let has_error = status_code.is_client_error() || status_code.is_server_error();
            if res.body.is_none()
                && !has_error
                && !status_code.is_redirection()
                && status_code != StatusCode::NO_CONTENT
                && status_code != StatusCode::SWITCHING_PROTOCOLS
                && [Method::GET, Method::POST, Method::PATCH, Method::PUT].contains(req.method())
            {
                // check for avoid warning when errors (404 etc.)
                tracing::warn!(
                    path = req.uri().path(),
                    method = req.method().as_str(),
                    "http response content type header not set"
                );
            }

            if Method::HEAD != *req.method()
                && (res.body.is_none() || res.body.is_error())
                && has_error
            {
                if let Some(catcher) = &state.catcher {
                    catcher
                        .catch(&mut req, &mut depot, &mut res, conn_ctrl)
                        .await;
                } else {
                    write_error_default(&req, &mut res, None);
                }
            }

            if Method::HEAD == *req.method() && !res.body.is_none() {
                tracing::debug!("stripping response body for HEAD request per RFC 9110 §9.3.2");
                res.take_body();
            }
            #[cfg(feature = "quinn")]
            {
                use std::sync::Mutex;

                use bytes::Bytes;
                if let Some(session) =
                    req.extensions.remove::<Arc<
                        crate::proto::WebTransportSession<salvo_http3::quinn::Connection, Bytes>,
                    >>()
                {
                    res.extensions.insert(session);
                }
                if let Some(conn) = req.extensions.remove::<Arc<
                    Mutex<salvo_http3::server::Connection<salvo_http3::quinn::Connection, Bytes>>,
                >>() {
                    res.extensions.insert(conn);
                }
                if let Some(stream) = req.extensions.remove::<Arc<
                    salvo_http3::server::RequestStream<
                        salvo_http3::quinn::BidiStream<Bytes>,
                        Bytes,
                    >,
                >>() {
                    res.extensions.insert(stream);
                }
            }
            // Middleware and catchers can hide a resource or provide their own
            // method policy. Do not retain inferred methods after a status
            // change, and never overwrite an application-provided Allow.
            if res.status_code == Some(StatusCode::METHOD_NOT_ALLOWED)
                && method_not_allowed
                && !res.headers().contains_key(http::header::ALLOW)
            {
                res.headers_mut()
                    .typed_insert(allowed.into_iter().collect::<Allow>());
            }
            #[cfg(feature = "response-observers")]
            response_observers.observe(&res);
            res
        }
    }
}

impl<B> HyperService<HyperRequest<B>> for HyperHandler
where
    B: Into<ReqBody>,
{
    type Response = HyperResponse<ResBody>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    #[inline]
    fn call(
        &self,
        #[cfg(not(feature = "fix-http1-request-uri"))] req: HyperRequest<B>,
        #[cfg(feature = "fix-http1-request-uri")] mut req: HyperRequest<B>,
    ) -> Self::Future {
        let scheme = req
            .uri()
            .scheme()
            .cloned()
            .unwrap_or_else(|| self.http_scheme.clone());
        // https://github.com/hyperium/hyper/issues/1310
        #[cfg(feature = "fix-http1-request-uri")]
        if req.uri().scheme().is_none()
            && let Some(host) = req
                .headers()
                .get(http::header::HOST)
                .and_then(|host| host.to_str().ok())
                .and_then(|host| host.parse::<http::uri::Authority>().ok())
        {
            let mut uri_parts = std::mem::take(req.uri_mut()).into_parts();
            uri_parts.scheme = Some(scheme.clone());
            uri_parts.authority = Some(host);
            if let Ok(uri) = http::uri::Uri::from_parts(uri_parts) {
                *req.uri_mut() = uri;
            }
        }
        let mut request = Request::from_hyper(req, scheme);
        request.body.set_fuse_config(self.fuse_config);
        let response = self.handle(request);
        let conn_ctrl = self.conn_ctrl.clone();
        Box::pin(async move {
            let response = response.await;
            // Do not return a response to Hyper after a handler abort. Yielding
            // Pending here lets the connection driver observe the abort signal
            // and drop the entire protocol connection first.
            if conn_ctrl.is_aborted() {
                pending::<()>().await;
            }
            Ok(response.into_hyper())
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::catcher::Catcher;
    use crate::prelude::*;
    use crate::test::{ResponseExt, TestClient};

    #[handler]
    async fn method_probe_goal() -> &'static str {
        "ok"
    }

    #[tokio::test]
    #[tracing_test::traced_test]
    async fn empty_response_warning_omits_query_secrets() {
        #[handler]
        async fn empty() {}
        let response = TestClient::get("http://localhost/warning-path?token=private-query-marker")
            .send(Router::with_path("warning-path").get(empty))
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert!(logs_contain("http response content type header not set"));
        assert!(logs_contain("/warning-path"));
        assert!(!logs_contain("private-query-marker"));
    }

    #[tokio::test]
    async fn rejected_media_type_uses_default_catcher_and_discards_body() {
        #[handler]
        async fn disallowed(req: &mut Request, res: &mut Response) {
            if req.uri().path() == "/error" {
                res.status_code(StatusCode::BAD_REQUEST);
            }
            if req.uri().path() != "/empty" {
                res.render("discarded original representation");
            }
            res.add_header("content-type", "application/json", true)
                .unwrap();
            // Metadata prepared by a compressor or proxy must not describe
            // the replacement error representation.
            res.add_header("content-encoding", "gzip", true).unwrap();
            res.add_header("content-length", "1", true).unwrap();
        }
        let service = Service::new(Router::with_path("{**path}").goal(disallowed))
            .allowed_media_types(vec![mime::TEXT_PLAIN]);
        for path in ["empty", "body", "error"] {
            let mut response = TestClient::get(format!("http://localhost/{path}"))
                .send(&service)
                .await;
            assert_eq!(
                response.status_code,
                Some(StatusCode::UNSUPPORTED_MEDIA_TYPE)
            );
            assert!(!response.headers().contains_key("content-encoding"));
            assert!(!response.headers().contains_key("content-length"));
            // ResponseExt decodes according to Content-Encoding, so this also
            // verifies that clients can read the replacement representation.
            let body = response.take_string().await.unwrap();
            assert!(body.contains("Unsupported Media Type"));
            assert!(!body.contains("discarded original representation"));
        }
        let response = TestClient::head("http://localhost/body")
            .send(&service)
            .await;
        assert_eq!(
            response.status_code,
            Some(StatusCode::UNSUPPORTED_MEDIA_TYPE)
        );
        assert!(response.body.is_none());
        assert!(!response.headers().contains_key("content-encoding"));
        assert!(!response.headers().contains_key("content-length"));
    }

    #[tokio::test]
    async fn rejected_media_type_runs_custom_catcher() {
        #[handler]
        async fn disallowed(req: &mut Request, res: &mut Response) {
            res.add_header("content-type", "application/json", true)
                .unwrap();
            res.render("discarded original representation");
            if req.uri().path() == "/encoded" {
                res.add_header("content-encoding", "gzip", true).unwrap();
                res.add_header("content-length", "1", true).unwrap();
            }
        }
        #[handler]
        async fn catch(res: &mut Response, ctrl: &mut FlowCtrl) {
            assert_eq!(res.status_code, Some(StatusCode::UNSUPPORTED_MEDIA_TYPE));
            assert!(!res.headers().contains_key("content-encoding"));
            assert!(!res.headers().contains_key("content-length"));
            res.render("custom media type error");
            ctrl.skip_rest();
        }
        let service = Service::new(Router::with_path("encoded").get(disallowed))
            .allowed_media_types(vec![mime::TEXT_PLAIN])
            .catcher(Catcher::default().hoop(catch));
        let mut response = TestClient::get("http://localhost/encoded")
            .send(&service)
            .await;
        assert_eq!(
            response.status_code,
            Some(StatusCode::UNSUPPORTED_MEDIA_TYPE)
        );
        assert_eq!(
            response.take_string().await.unwrap(),
            "custom media type error"
        );

        let mut allowed = TestClient::get("http://localhost/")
            .send(
                &Service::new(Router::new().get(disallowed))
                    .allowed_media_types(vec![mime::APPLICATION_JSON]),
            )
            .await;
        assert_eq!(allowed.status_code, Some(StatusCode::OK));
        assert_eq!(
            allowed.take_string().await.unwrap(),
            "discarded original representation"
        );
    }

    #[tokio::test]
    async fn automatic_405_lists_methods_for_the_matched_resource() {
        use crate::routing::filters::MethodFilter;

        let router = Router::new()
            .push(
                Router::with_path("items")
                    .get(method_probe_goal)
                    .post(method_probe_goal)
                    .get(method_probe_goal)
                    .push(
                        Router::with_filter(MethodFilter::new("PROPFIND".parse().unwrap()))
                            .goal(method_probe_goal),
                    ),
            )
            .push(Router::with_path("other").put(method_probe_goal));
        let service = Service::new(router);
        let response = TestClient::delete("http://localhost/items")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::METHOD_NOT_ALLOWED));
        assert_eq!(
            response.headers().get("allow").unwrap(),
            "GET, POST, PROPFIND"
        );
        let response = TestClient::get("http://localhost/items")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert!(response.headers().get("allow").is_none());
    }

    #[tokio::test]
    async fn automatic_405_keeps_the_original_path_after_a_filter_rewrites_the_uri() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        for (original, rewritten, status, allow) in [
            ("missing", "items", StatusCode::NOT_FOUND, None),
            (
                "items",
                "missing",
                StatusCode::METHOD_NOT_ALLOWED,
                Some("GET"),
            ),
        ] {
            let calls = Arc::new(AtomicUsize::new(0));
            let observed = calls.clone();
            let router = Router::new()
                .push(Router::new().filter_fn(move |req, _| {
                    observed.fetch_add(1, Ordering::Relaxed);
                    *req.uri_mut() = format!("http://localhost/{rewritten}").parse().unwrap();
                    false
                }))
                .push(Router::with_path("items").get(method_probe_goal));
            let response = TestClient::delete(format!("http://localhost/{original}"))
                .send(router)
                .await;
            assert_eq!(response.status_code, Some(status));
            assert_eq!(
                response
                    .headers()
                    .get("allow")
                    .map(|value| value.to_str().unwrap()),
                allow,
            );
            assert_eq!(calls.load(Ordering::Relaxed), 1);
        }
    }

    #[tokio::test]
    async fn automatic_405_checks_filters_after_methods_and_requires_a_goal() {
        use crate::http::uri::Scheme;
        use crate::routing::filters;

        let router = Router::new()
            .push(
                Router::with_filter(filters::get())
                    .path("files/{id:num}")
                    .host("example.com")
                    .port(8080)
                    .scheme(Scheme::HTTPS)
                    .goal(method_probe_goal),
            )
            .push(
                Router::with_filter(filters::post())
                    .path("files/missing")
                    .goal(method_probe_goal),
            )
            .push(Router::with_filter(filters::delete()).path("files/{id:num}"));
        let service = Service::new(router);
        let response = TestClient::put("https://example.com:8080/files/12")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::METHOD_NOT_ALLOWED));
        assert_eq!(response.headers().get("allow").unwrap(), "GET");
        let response = TestClient::put("https://other.example:8080/files/12")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NOT_FOUND));
        assert!(response.headers().get("allow").is_none());

        let response = TestClient::put("http://localhost/a")
            .send(Router::with_path("a"))
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NOT_FOUND));
        assert!(response.headers().get("allow").is_none());
    }

    #[tokio::test]
    async fn automatic_405_understands_composite_method_filters() {
        use crate::routing::{Filter, filters};

        let filter = filters::get()
            .and(filters::path("a"))
            .or(filters::post().and(filters::path("a")));
        let response = TestClient::delete("http://localhost/a")
            .send(Router::with_filter(filter).goal(method_probe_goal))
            .await;
        assert_eq!(response.status_code, Some(StatusCode::METHOD_NOT_ALLOWED));
        assert_eq!(response.headers().get("allow").unwrap(), "GET, POST");
    }

    #[tokio::test]
    async fn automatic_405_does_not_replay_opaque_callbacks() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use crate::http::Method;
        use crate::routing::filters::{self, CharsWispBuilder, PathFilter};

        let called = Arc::new(AtomicUsize::new(0));
        let callback_called = called.clone();
        let chars_called = called.clone();
        PathFilter::register_wisp_builder(
            "allow_probe_counter",
            CharsWispBuilder::new(move |_| {
                chars_called.fetch_add(1, Ordering::Relaxed);
                true
            }),
        );
        let router = Router::new()
            .push(
                Router::with_filter(filters::get())
                    .path("a")
                    .filter_fn(move |req, _| {
                        assert_eq!(req.method(), Method::DELETE);
                        callback_called.fetch_add(1, Ordering::Relaxed);
                        true
                    })
                    .goal(method_probe_goal),
            )
            .push(
                Router::with_filter(filters::put())
                    .path("{id:allow_probe_counter}")
                    .goal(method_probe_goal),
            )
            .push(Router::with_path("a").post(method_probe_goal));
        let response = TestClient::delete("http://localhost/a").send(router).await;
        assert_eq!(response.status_code, Some(StatusCode::METHOD_NOT_ALLOWED));
        assert_eq!(response.headers().get("allow").unwrap(), "POST");
        assert_eq!(called.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn automatic_405_does_not_treat_unknown_as_a_failed_or_branch() {
        use crate::routing::{Filter, filters};

        let shadowed = filters::FnFilter(
            |_: &mut Request, state: &mut crate::routing::PathState<'_>| {
                state.forward(2);
                true
            },
        )
        .or(filters::path("a"))
        .and(filters::path("b"));
        let router = Router::new()
            .push(
                Router::with_filter(filters::get())
                    .filter(shadowed)
                    .goal(method_probe_goal),
            )
            .push(Router::with_path("a/b").post(method_probe_goal));
        let response = TestClient::delete("http://localhost/a/b")
            .send(router)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::METHOD_NOT_ALLOWED));
        assert_eq!(response.headers().get("allow").unwrap(), "POST");
    }

    #[tokio::test]
    async fn automatic_405_preserves_nested_method_constraints() {
        use crate::routing::filters;

        let router = Router::new()
            .push(
                Router::with_filter(filters::get())
                    .push(Router::with_path("a").get(method_probe_goal)),
            )
            .push(
                Router::with_filter(filters::patch())
                    .push(Router::with_path("a").post(method_probe_goal)),
            );
        let response = TestClient::delete("http://localhost/a").send(router).await;
        assert_eq!(response.status_code, Some(StatusCode::METHOD_NOT_ALLOWED));
        assert_eq!(response.headers().get("allow").unwrap(), "GET");
    }

    #[tokio::test]
    async fn automatic_405_defers_to_service_middleware() {
        use crate::http::Method;

        #[handler]
        async fn hide(req: &mut Request, res: &mut Response) {
            assert_eq!(req.method(), Method::DELETE);
            res.status_code(StatusCode::NOT_FOUND);
        }
        #[handler]
        async fn explicit(req: &mut Request, res: &mut Response) {
            assert_eq!(req.method(), Method::DELETE);
            res.status_code(StatusCode::METHOD_NOT_ALLOWED);
            res.headers_mut()
                .insert("allow", "OPTIONS".parse().unwrap());
        }

        let service = Service::new(Router::with_path("a").get(method_probe_goal)).hoop(hide);
        let response = TestClient::delete("http://localhost/a")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NOT_FOUND));
        assert!(response.headers().get("allow").is_none());

        let service = Service::new(Router::with_path("a").get(method_probe_goal)).hoop(explicit);
        let response = TestClient::delete("http://localhost/a")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::METHOD_NOT_ALLOWED));
        assert_eq!(response.headers().get("allow").unwrap(), "OPTIONS");
    }

    #[tokio::test]
    async fn test_service() {
        #[handler]
        async fn before1(
            req: &mut Request,
            depot: &mut Depot,
            res: &mut Response,
            ctrl: &mut FlowCtrl,
        ) {
            res.render(Text::Plain("before1"));
            if req.query::<String>("b").unwrap_or_default() == "1" {
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        }
        #[handler]
        async fn before2(
            req: &mut Request,
            depot: &mut Depot,
            res: &mut Response,
            ctrl: &mut FlowCtrl,
        ) {
            res.render(Text::Plain("before2"));
            if req.query::<String>("b").unwrap_or_default() == "2" {
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        }
        #[handler]
        async fn before3(
            req: &mut Request,
            depot: &mut Depot,
            res: &mut Response,
            ctrl: &mut FlowCtrl,
        ) {
            res.render(Text::Plain("before3"));
            if req.query::<String>("b").unwrap_or_default() == "3" {
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        }
        #[handler]
        async fn hello() -> Result<&'static str, ()> {
            Ok("hello")
        }
        let router = Router::with_path("level1").hoop(before1).push(
            Router::with_hoop(before2)
                .path("level2")
                .push(Router::with_hoop(before3).path("hello").goal(hello)),
        );
        let service = Service::new(router);

        async fn access(service: &Service, b: &str) -> String {
            TestClient::get(format!("http://127.0.0.1:5801/level1/level2/hello?b={b}"))
                .send(service)
                .await
                .take_string()
                .await
                .unwrap()
        }
        let content = access(&service, "").await;
        assert_eq!(content, "before1before2before3hello");
        let content = access(&service, "1").await;
        assert_eq!(content, "before1");
        let content = access(&service, "2").await;
        assert_eq!(content, "before1before2");
        let content = access(&service, "3").await;
        assert_eq!(content, "before1before2before3");
    }

    #[tokio::test]
    async fn test_service_405_or_404_error() {
        #[handler]
        async fn login() -> &'static str {
            "login"
        }
        #[handler]
        async fn hello() -> &'static str {
            "hello"
        }
        let router = Router::new()
            .push(Router::with_path("hello").goal(hello))
            .push(
                Router::with_path("login")
                    .post(login)
                    .push(Router::with_path("user").get(login)),
            );
        let service = Service::new(router);

        let res = TestClient::get("http://127.0.0.1:5801/hello")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);
        let res = TestClient::put("http://127.0.0.1:5801/hello")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let res = TestClient::post("http://127.0.0.1:5801/login")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let res = TestClient::get("http://127.0.0.1:5801/login")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::METHOD_NOT_ALLOWED);

        let res = TestClient::get("http://127.0.0.1:5801/login2")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::NOT_FOUND);

        let res = TestClient::get("http://127.0.0.1:5801/login/user")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let res = TestClient::post("http://127.0.0.1:5801/login/user")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::METHOD_NOT_ALLOWED);

        let res = TestClient::post("http://127.0.0.1:5801/login/user1")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::NOT_FOUND);
    }

    /// Regression test for [#925](https://github.com/salvo-rs/salvo/pull/925).
    #[tokio::test]
    async fn test_default_status_ok_visible_to_service_middleware() {
        #[handler]
        async fn goal(res: &mut Response) {
            res.render("ok");
        }

        #[handler]
        async fn check_status(
            req: &mut Request,
            depot: &mut Depot,
            res: &mut Response,
            ctrl: &mut FlowCtrl,
        ) {
            ctrl.call_next(req, depot, res).await;

            assert_eq!(
                res.status_code,
                Some(StatusCode::OK),
                "service middleware should observe implicit 200 after call_next"
            );
        }

        let router = Router::new().goal(goal);

        let service = Service::new(router).hoop(check_status);

        let res = TestClient::get("http://127.0.0.1:5802/")
            .send(&service)
            .await;

        assert_eq!(res.status_code, Some(StatusCode::OK));
    }
}
