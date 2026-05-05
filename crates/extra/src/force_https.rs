//! Middleware force redirect to https.
//!
//! The force-https middleware can force all requests to use the HTTPS protocol.
//!
//! If this middleware is applied to the Router, the protocol will be forced to
//! convert only when the route is matched. If the page does not exist, it will
//! not be redirected.
//!
//! But the more common requirement is to expect any request to be
//! automatically redirected, even when the route fails to match and returns a
//! 404 error. At this time, the middleware can be added to the Service.
//! Regardless of whether the request is successfully matched by the route,
//! the middleware added to the Service will always be executed.
//!
//! Example:
//!
//! ```no_run
//! use salvo_core::prelude::*;
//! use salvo_core::conn::rustls::{Keycert, RustlsConfig};
//! use salvo_extra::force_https::ForceHttps;
//!
//! #[handler]
//! async fn hello() -> &'static str {
//!     "hello"
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::new().get(hello);
//!     let service = Service::new(router).hoop(ForceHttps::new().https_port(5443));
//!
//!     let config = RustlsConfig::new(
//!         Keycert::new()
//!             .cert(include_bytes!("../../core/certs/cert.pem").as_ref())
//!             .key(include_bytes!("../../core/certs/key.pem").as_ref()),
//!     );
//!     let acceptor = TcpListener::new("0.0.0.0:5443")
//!         .rustls(config)
//!         .join(TcpListener::new("0.0.0.0:8698"))
//!         .bind()
//!         .await;
//!     Server::new(acceptor).serve(service).await;
//! }
//! ```
use std::borrow::Cow;
use std::fmt::{self, Debug, Formatter};
use std::sync::atomic::{AtomicBool, Ordering};

use salvo_core::handler::Skipper;
use salvo_core::http::header;
use salvo_core::http::uri::{Scheme, Uri};
use salvo_core::http::{Request, ResBody, Response};
use salvo_core::writing::Redirect;
use salvo_core::{Depot, FlowCtrl, Handler, async_trait};

/// Middleware for force redirect to http uri.
#[derive(Default)]
pub struct ForceHttps {
    https_port: Option<u16>,
    canonical_host: Option<String>,
    trust_host_header: bool,
    skipper: Option<Box<dyn Skipper>>,
    no_op_warned: AtomicBool,
}

impl Debug for ForceHttps {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ForceHttps")
            .field("https_port", &self.https_port)
            .field("canonical_host", &self.canonical_host)
            .field("trust_host_header", &self.trust_host_header)
            .finish()
    }
}

impl ForceHttps {
    /// Create new `ForceHttps` middleware.
    #[must_use]
    pub fn new() -> Self {
        Default::default()
    }

    /// Specify https port.
    #[must_use]
    pub fn https_port(self, port: u16) -> Self {
        Self {
            https_port: Some(port),
            ..self
        }
    }

    /// Specify the public host used in redirect locations.
    #[must_use]
    pub fn canonical_host(self, host: impl Into<String>) -> Self {
        Self {
            canonical_host: Some(host.into()),
            ..self
        }
    }

    /// Trust the request `Host`/`:authority` value when building redirect locations.
    ///
    /// Prefer [`ForceHttps::canonical_host`] for public services. Enable this
    /// only when a trusted proxy or edge layer validates and overwrites host
    /// headers before requests reach the application.
    #[must_use]
    pub fn trust_host_header(self, trust: bool) -> Self {
        Self {
            trust_host_header: trust,
            ..self
        }
    }

    /// Uses a closure to determine if a request should be redirect.
    #[must_use]
    pub fn skipper(self, skipper: impl Skipper) -> Self {
        Self {
            skipper: Some(Box::new(skipper)),
            ..self
        }
    }
}

#[async_trait]
impl Handler for ForceHttps {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        if req.uri().scheme() == Some(&Scheme::HTTPS)
            || self
                .skipper
                .as_ref()
                .map(|skipper| skipper.skipped(req, depot))
                .unwrap_or(false)
        {
            return;
        }
        let redirect_base_host = if let Some(host) = self.canonical_host.as_deref() {
            Some(Cow::Borrowed(host))
        } else if self.trust_host_header {
            // Prefer the `Host` header: that is what trusted proxies validate
            // and rewrite. Falling back to `req.uri().authority()` first would
            // let an HTTP/1.1 absolute-form request like
            // `GET http://evil.example/ HTTP/1.1` bypass a validated Host
            // value and steer the redirect target. Fall back to `:authority`
            // only when no `Host` header is present (HTTP/2 deployments).
            req.header::<String>(header::HOST)
                .map(Cow::Owned)
                .or_else(|| {
                    req.uri()
                        .authority()
                        .map(|authority| Cow::Owned(authority.as_str().to_owned()))
                })
        } else {
            if !self.no_op_warned.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    "ForceHttps has neither `canonical_host(...)` nor `trust_host_header(true)` \
                     configured; non-HTTPS requests will not be redirected. Set \
                     `canonical_host(...)` for public services, or call `trust_host_header(true)` \
                     only when a trusted proxy validates the Host/:authority header."
                );
            }
            None
        };

        if let Some(host) = redirect_base_host {
            let host = redirect_host(&host, self.https_port);
            let uri_parts = std::mem::take(req.uri_mut()).into_parts();
            let mut builder = Uri::builder().scheme(Scheme::HTTPS).authority(&*host);
            if let Some(path_and_query) = uri_parts.path_and_query {
                builder = builder.path_and_query(path_and_query);
            }
            if let Ok(uri) = builder.build() {
                res.body(ResBody::None);
                res.render(Redirect::permanent(uri.to_string()));
                ctrl.skip_rest();
            }
        }
    }
}

fn redirect_host(host: &str, https_port: Option<u16>) -> Cow<'_, str> {
    match (host.split_once(':'), https_port) {
        (Some((host, _)), Some(port)) => Cow::Owned(format!("{host}:{port}")),
        (None, Some(port)) => Cow::Owned(format!("{host}:{port}")),
        (_, None) => Cow::Borrowed(host),
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::http::header::{HOST, LOCATION};
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use super::*;

    #[test]
    fn test_redirect_host() {
        assert_eq!(redirect_host("example.com", Some(1234)), "example.com:1234");
        assert_eq!(
            redirect_host("example.com:5678", Some(1234)),
            "example.com:1234"
        );
        assert_eq!(redirect_host("example.com", Some(1234)), "example.com:1234");
        assert_eq!(redirect_host("example.com:1234", None), "example.com:1234");
        assert_eq!(redirect_host("example.com", None), "example.com");
    }

    #[handler]
    async fn hello() -> &'static str {
        "Hello World"
    }
    #[tokio::test]
    async fn test_redirect_handler() {
        let router =
            Router::with_hoop(ForceHttps::new().https_port(1234).trust_host_header(true))
                .goal(hello);
        let response = TestClient::get("http://127.0.0.1:8698/")
            .add_header(HOST, "127.0.0.1:8698", true)
            .send(router)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::PERMANENT_REDIRECT));
        assert_eq!(
            response.headers().get(LOCATION),
            Some(&"https://127.0.0.1:1234/".parse().unwrap())
        );
    }

    #[tokio::test]
    async fn test_redirect_handler_does_not_trust_host_by_default() {
        let router = Router::with_hoop(ForceHttps::new()).goal(hello);
        let mut response = TestClient::get("http://127.0.0.1:8698/")
            .add_header(HOST, "attacker.example", true)
            .send(router)
            .await;

        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.headers().get(LOCATION), None);
        assert_eq!(response.take_string().await.unwrap(), "Hello World");
    }

    #[tokio::test]
    async fn test_redirect_handler_prefers_host_over_uri_authority() {
        // Simulate an HTTP/1.1 absolute-form request where the request-line
        // authority points at an attacker-controlled host but the trusted
        // proxy has validated/rewritten the `Host` header to a safe value.
        // The redirect must follow the validated `Host`, not the URI.
        let router =
            Router::with_hoop(ForceHttps::new().trust_host_header(true)).goal(hello);
        let response = TestClient::get("http://evil.example/")
            .add_header(HOST, "public.example", true)
            .send(router)
            .await;

        assert_eq!(response.status_code, Some(StatusCode::PERMANENT_REDIRECT));
        assert_eq!(
            response.headers().get(LOCATION),
            Some(&"https://public.example/".parse().unwrap())
        );
    }

    #[tokio::test]
    async fn test_redirect_handler_uses_canonical_host() {
        let router = Router::with_hoop(ForceHttps::new().canonical_host("public.example.com"))
            .goal(hello);
        let response = TestClient::get("http://127.0.0.1:8698/")
            .add_header(HOST, "attacker.example", true)
            .send(router)
            .await;

        assert_eq!(response.status_code, Some(StatusCode::PERMANENT_REDIRECT));
        assert_eq!(
            response.headers().get(LOCATION),
            Some(&"https://public.example.com/".parse().unwrap())
        );
    }
}
