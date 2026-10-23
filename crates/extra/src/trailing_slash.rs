//! Trailing slash middleware.
//!
//! # Examples
//!
//! - Add trailing slash:
//!
//! ```no_run
//! use salvo_core::prelude::*;
//! use salvo_extra::trailing_slash::add_slash;
//!
//! #[handler]
//! async fn hello() -> &'static str {
//!     "Hello"
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::with_hoop(add_slash())
//!         .push(Router::with_path("hello").get(hello))
//!         .push(Router::with_path("hello.world").get(hello));
//!     let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```
//!
//! - Remove trailing slash:
//!
//! ```no_run
//! use salvo_core::prelude::*;
//! use salvo_extra::trailing_slash::remove_slash;
//!
//! #[handler]
//! async fn hello() -> &'static str {
//!     "Hello"
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::with_hoop(remove_slash().redirect_code(StatusCode::TEMPORARY_REDIRECT))
//!         .push(Router::with_path("hello").get(hello))
//!         .push(Router::with_path("hello.world").get(hello));
//!     let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```
use std::borrow::Cow;
use std::fmt::{self, Debug, Formatter};
use std::str::FromStr;

use salvo_core::handler::Skipper;
use salvo_core::http::uri::{PathAndQuery, Uri};
use salvo_core::http::{ParseError, ResBody};
use salvo_core::prelude::*;

/// TrailingSlashAction
#[derive(Eq, PartialEq, Clone, Debug, Copy)]
pub enum TrailingSlashAction {
    /// Remove trailing slash.
    Remove,
    /// Add trailing slash.
    Add,
}

/// Default skipper used for `TrailingSlash` when its action is [`TrailingSlashAction::Remove`].
pub fn default_remove_skipper(req: &mut Request, _depot: &Depot) -> bool {
    if let Some((_, name)) = req.uri().path().trim_end_matches('/').rsplit_once('/') {
        !name.contains('.')
    } else {
        false
    }
}

/// Default skipper used for `TrailingSlash` when its action is [`TrailingSlashAction::Add`].
pub fn default_add_skipper(req: &mut Request, _depot: &Depot) -> bool {
    if let Some((_, name)) = req.uri().path().rsplit_once('/') {
        name.contains('.')
    } else {
        false
    }
}

/// Middleware that adds, removes, or redirects trailing slashes on request paths.
#[non_exhaustive]
pub struct TrailingSlash {
    /// Action to perform on the trailing slash.
    pub action: TrailingSlashAction,
    /// Skip adding or removing the trailing slash when this skipper returns `true`.
    pub skipper: Box<dyn Skipper>,
    /// HTTP status code used when redirecting to the canonical URL.
    pub redirect_code: StatusCode,
}

impl Debug for TrailingSlash {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrailingSlash")
            .field("action", &self.action)
            .field("redirect_code", &self.redirect_code)
            .finish()
    }
}

impl TrailingSlash {
    /// Creates a new `TrailingSlash`.
    #[inline]
    pub fn new(action: TrailingSlashAction) -> Self {
        Self {
            action,
            skipper: match action {
                TrailingSlashAction::Add => Box::new(default_add_skipper),
                TrailingSlashAction::Remove => Box::new(default_remove_skipper),
            },
            redirect_code: StatusCode::MOVED_PERMANENTLY,
        }
    }
    /// Creates a new `TrailingSlash` and sets its action to [`TrailingSlashAction::Add`].
    #[inline]
    #[must_use]
    pub fn new_add() -> Self {
        Self::new(TrailingSlashAction::Add)
    }
    /// Creates a new `TrailingSlash` and sets its action to [`TrailingSlashAction::Remove`].
    #[inline]
    #[must_use]
    pub fn new_remove() -> Self {
        Self::new(TrailingSlashAction::Remove)
    }
    /// Sets skipper and returns new `TrailingSlash`.
    #[inline]
    #[must_use]
    pub fn skipper(mut self, skipper: impl Skipper) -> Self {
        self.skipper = Box::new(skipper);
        self
    }

    /// Sets redirect code and returns new `TrailingSlash`.
    #[inline]
    #[must_use]
    pub fn redirect_code(mut self, redirect_code: StatusCode) -> Self {
        self.redirect_code = redirect_code;
        self
    }
}

#[async_trait]
impl Handler for TrailingSlash {
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

        let original_path = req.uri().path();
        if !original_path.is_empty() && original_path != "/" {
            // skip root path
            let ends_with_slash = original_path.ends_with('/');
            let new_uri = if self.action == TrailingSlashAction::Add && !ends_with_slash {
                replace_uri_path(req.uri(), &format!("{original_path}/")).ok()
            } else if self.action == TrailingSlashAction::Remove && ends_with_slash {
                replace_uri_path(req.uri(), original_path.trim_end_matches('/')).ok()
            } else {
                None
            };
            if let Some(new_uri) = new_uri {
                ctrl.skip_rest();
                res.body(ResBody::None);
                match Redirect::with_status_code(self.redirect_code, new_uri.to_string()) {
                    Ok(redirect) => {
                        res.render(redirect);
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "redirect failed");
                    }
                }
            }
        }
    }
}

fn replace_uri_path(original_uri: &Uri, new_path: &str) -> Result<Uri, ParseError> {
    let mut uri_parts = original_uri.clone().into_parts();
    // A relative Location beginning with two separators is interpreted by
    // browsers as a new authority (including /\\ on HTTP URLs). A dot segment
    // keeps it origin-relative while preserving the path after URL resolution.
    // Do not rely on the optional HTTP/1 URI fix to supply an authority.
    let new_path = if original_uri.authority().is_none()
        && (new_path.starts_with("//") || new_path.starts_with("/\\"))
    {
        Cow::Owned(format!("/.{new_path}"))
    } else {
        Cow::Borrowed(new_path)
    };
    let path = match original_uri.query() {
        Some(query) => Cow::from(format!("{new_path}?{query}")),
        None => new_path,
    };
    uri_parts.path_and_query = Some(PathAndQuery::from_str(path.as_ref())?);
    Ok(Uri::from_parts(uri_parts)?)
}

/// Create an add slash middleware.
#[inline]
#[must_use]
pub fn add_slash() -> TrailingSlash {
    TrailingSlash::new(TrailingSlashAction::Add)
}

/// Create a remove slash middleware.
#[inline]
#[must_use]
pub fn remove_slash() -> TrailingSlash {
    TrailingSlash::new(TrailingSlashAction::Remove)
}

#[cfg(test)]
mod tests {
    use salvo_core::http::StatusCode;
    use salvo_core::test::TestClient;

    use super::*;

    #[test]
    fn relative_redirects_preserve_origin_and_path() {
        let base = url::Url::parse("http://trusted.example/current").unwrap();
        for path in ["/hello", "//evil.example/path", "///evil.example/path", "/\\evil.example/path"] {
            let original: Uri = format!("{path}?next=%2Faccount").parse().unwrap();
            let target = replace_uri_path(&original, &format!("{path}/")).unwrap();
            let resolved = base.join(&target.to_string()).unwrap();
            assert_eq!(resolved.origin(), base.origin(), "{target}");
            assert_eq!(resolved.path(), format!("{}/", path.replace('\\', "/")));
            assert_eq!(resolved.query(), Some("next=%2Faccount"));
        }
        let original: Uri = "http://trusted.example//path?q=1".parse().unwrap();
        assert_eq!(
            replace_uri_path(&original, "//path/").unwrap().to_string(),
            "http://trusted.example//path/?q=1"
        );
    }

    #[tokio::test]
    async fn hyper_redirects_stay_same_origin_with_or_without_uri_fix() {
        use salvo_core::hyper::service::Service as _;
        use salvo_core::http::ReqBody;

        for (action, input, expected_path) in [
            (TrailingSlashAction::Add, "//evil.example/path", "//evil.example/path/"),
            (TrailingSlashAction::Remove, "//evil.example/path/", "//evil.example/path"),
        ] {
            let service = Service::new(Router::with_hoop(
                TrailingSlash::new(action).skipper(|_: &mut Request, _: &Depot| false),
            ).push(Router::with_path("{**rest}").goal(hello)));
            let handler = service.hyper_handler(
                "127.0.0.1:80".parse::<std::net::SocketAddr>().unwrap().into(),
                "127.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap().into(),
                salvo_core::http::uri::Scheme::HTTP,
                None,
                salvo_core::ConnCtrl::new(),
                None,
            );
            let request = salvo_core::hyper::Request::builder()
                .uri(format!("{input}?q=1"))
                .header("host", "trusted.example")
                .body(ReqBody::None)
                .unwrap();
            let response = handler.call(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::MOVED_PERMANENTLY);
            let base = url::Url::parse("http://trusted.example/").unwrap();
            let location = response.headers()["location"].to_str().unwrap();
            let resolved = base.join(location).unwrap();
            assert_eq!(resolved.origin(), base.origin());
            assert_eq!(resolved.path(), expected_path);
            assert_eq!(resolved.query(), Some("q=1"));
        }
    }

    #[handler]
    async fn hello() -> &'static str {
        "Hello World"
    }
    #[tokio::test]
    async fn test_add_slash() {
        let router = Router::with_hoop(add_slash())
            .push(Router::with_path("hello").get(hello))
            .push(Router::with_path("hello.world").get(hello));
        let service = Service::new(router);
        let res = TestClient::get("http://127.0.0.1:8698/hello")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::MOVED_PERMANENTLY);

        let res = TestClient::get("http://127.0.0.1:8698/hello/")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let res = TestClient::get("http://127.0.0.1:8698/hello.world")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);
    }
    #[tokio::test]
    async fn test_remove_slash() {
        let router =
            Router::with_hoop(remove_slash().redirect_code(StatusCode::TEMPORARY_REDIRECT))
                .push(Router::with_path("hello").get(hello))
                .push(Router::with_path("hello.world").get(hello));
        let service = Service::new(router);
        let res = TestClient::get("http://127.0.0.1:8698/hello/")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let res = TestClient::get("http://127.0.0.1:8698/hello.world/")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::TEMPORARY_REDIRECT);

        let res = TestClient::get("http://127.0.0.1:8698/hello.world")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);
    }
}
