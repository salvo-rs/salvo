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
//!         .join(TcpListener::new("0.0.0.0:5800"))
//!         .bind()
//!         .await;
//!     Server::new(acceptor).serve(service).await;
//! }
//! ```
use std::borrow::Cow;

use salvo_core::handler::Skipper;
use salvo_core::http::header;
use salvo_core::http::uri::{Scheme, Uri};
use salvo_core::http::{Request, ResBody, Response};
use salvo_core::writing::Redirect;
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};

/// Middleware for force redirect to http uri.
#[derive(Default)]
pub struct ForceHttps {
    https_port: Option<u16>,
    skipper: Option<Box<dyn Skipper>>,
}
impl ForceHttps {
    /// Create new `ForceHttps` middleware.
    pub fn new() -> Self {
        Default::default()
    }

    /// Specify https port.
    pub fn https_port(self, port: u16) -> Self {
        Self {
            https_port: Some(port),
            ..self
        }
    }

    /// Uses a closure to determine if a request should be redirect.
    pub fn skipper(self, skipper: impl Skipper) -> Self {
        Self {
            skipper: Some(Box::new(skipper)),
            ..self
        }
    }
}

#[async_trait]
impl Handler for ForceHttps {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if req.uri().scheme() == Some(&Scheme::HTTPS)
            || self
                .skipper
                .as_ref()
                .map(|skipper| skipper.skipped(req, depot))
                .unwrap_or(false)
        {
            return;
        }
        if let Some(host) = req.header::<String>(header::HOST) {
            let host = redirect_host(&host, self.https_port);
            let uri_parts = std::mem::take(req.uri_mut()).into_parts();
            let mut builder = Uri::builder().scheme(Scheme::HTTPS).authority(&*host);
            if let Some(path_and_query) = uri_parts.path_and_query {
                builder = builder.path_and_query(path_and_query);
            }
            if let Ok(uri) = builder.build() {
                res.body(ResBody::None);
                res.render(Redirect::permanent(uri));
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
    use salvo_core::test::TestClient;

    use super::*;

    #[test]
    fn test_redirect_host() {
        assert_eq!(redirect_host("example.com", Some(1234)), "example.com:1234");
        assert_eq!(redirect_host("example.com:5678", Some(1234)), "example.com:1234");
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
        let router = Router::with_hoop(ForceHttps::new().https_port(1234)).goal(hello);
        let response = TestClient::get("http://127.0.0.1:5800/")
            .add_header(HOST, "127.0.0.1:5800", true)
            .send(router)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::PERMANENT_REDIRECT));
        assert_eq!(
            response.headers().get(LOCATION),
            Some(&"https://127.0.0.1:1234/".parse().unwrap())
        );
    }
}
