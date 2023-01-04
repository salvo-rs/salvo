//! Trailing slash middleware

use std::borrow::Cow;
use std::str::FromStr;

use salvo_core::handler::Skipper;
use salvo_core::http::uri::{PathAndQuery, Uri};
use salvo_core::http::ResBody;
use salvo_core::prelude::*;

/// TrailingSlashAction
#[derive(Eq, PartialEq, Clone, Debug, Copy)]
pub enum TrailingSlashAction {
    /// Remove trailing slash.
    Remove,
    /// Add trailing slash.
    Add,
}

/// Default skipper used for `TrailingSlash` when it's action is [`TrailingSlashAction::Remove`].
pub fn default_remove_skipper(req: &mut Request, _depot: &Depot) -> bool {
    if let Some((_, name)) = req.uri().path().trim_end_matches('/').rsplit_once('/') {
        !name.contains('.')
    } else {
        false
    }
}

/// Default skipper used for `TrailingSlash` when it's action is [`TrailingSlashAction::Add`].
pub fn default_add_skipper(req: &mut Request, _depot: &Depot) -> bool {
    if let Some((_, name)) = req.uri().path().rsplit_once('/') {
        name.contains('.')
    } else {
        false
    }
}

/// TrailingSlash
pub struct TrailingSlash {
    /// Action of this `TrailingSlash`.
    pub action: TrailingSlashAction,
    /// Skip to Remove or add slash when skipper is returns `true`.
    pub skipper: Box<dyn Skipper>,
    /// Redirect code is used when redirect url.
    pub redirect_code: StatusCode,
}
impl TrailingSlash {
    /// Create new `TrailingSlash`.
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
    /// Create new `TrailingSlash` and sets it's action as [`TrailingSlashAction::Add`].
    #[inline]
    pub fn new_add() -> Self {
        Self::new(TrailingSlashAction::Add)
    }
    /// Create new `TrailingSlash` and sets it's action as [`TrailingSlashAction::Remove`].
    #[inline]
    pub fn new_remove() -> Self {
        Self::new(TrailingSlashAction::Remove)
    }
    /// Sets skipper and returns new `TrailingSlash`.
    #[inline]
    pub fn with_skipper(mut self, skipper: impl Skipper) -> Self {
        self.skipper = Box::new(skipper);
        self
    }

    /// Sets redirect code and returns new `TrailingSlash`.
    #[inline]
    pub fn with_redirect_code(mut self, redirect_code: StatusCode) -> Self {
        self.redirect_code = redirect_code;
        self
    }
}

#[async_trait]
impl Handler for TrailingSlash {
    #[inline]
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if self.skipper.skipped(req, depot) {
            return;
        }

        let original_path = req.uri().path();
        if !original_path.is_empty() {
            let ends_with_slash = original_path.ends_with('/');
            let new_uri = if self.action == TrailingSlashAction::Add && !ends_with_slash {
                Some(replace_uri_path(req.uri(), &format!("{original_path}/")))
            } else if self.action == TrailingSlashAction::Remove && ends_with_slash {
                Some(replace_uri_path(req.uri(), original_path.trim_end_matches('/')))
            } else {
                None
            };
            if let Some(new_uri) = new_uri {
                ctrl.skip_rest();
                res.set_body(ResBody::None);
                match Redirect::with_status_code(self.redirect_code, new_uri) {
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

#[inline]
fn replace_uri_path(original_uri: &Uri, new_path: &str) -> Uri {
    let mut uri_parts = original_uri.clone().into_parts();
    let path = match original_uri.query() {
        Some(query) => Cow::from(format!("{new_path}?{query}")),
        None => Cow::from(new_path),
    };
    uri_parts.path_and_query = Some(PathAndQuery::from_str(path.as_ref()).unwrap());
    Uri::from_parts(uri_parts).unwrap()
}

/// Create an add slash middleware.
#[inline]
pub fn add_slash() -> TrailingSlash {
    TrailingSlash::new(TrailingSlashAction::Add)
}

/// Create a remove slash middleware.
#[inline]
pub fn remove_slash() -> TrailingSlash {
    TrailingSlash::new(TrailingSlashAction::Remove)
}

#[cfg(test)]
mod tests {
    use salvo_core::http::StatusCode;
    use salvo_core::prelude::*;
    use salvo_core::test::TestClient;

    use super::*;

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
        let res = TestClient::get("http://127.0.0.1:7878/hello").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::MOVED_PERMANENTLY);

        let res = TestClient::get("http://127.0.0.1:7878/hello/").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let res = TestClient::get("http://127.0.0.1:7878/hello.world")
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
    }
    #[tokio::test]
    async fn test_remove_slash() {
        let router = Router::with_hoop(remove_slash().with_redirect_code(StatusCode::TEMPORARY_REDIRECT))
            .push(Router::with_path("hello").get(hello))
            .push(Router::with_path("hello.world").get(hello));
        let service = Service::new(router);
        let res = TestClient::get("http://127.0.0.1:7878/hello/").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let res = TestClient::get("http://127.0.0.1:7878/hello.world/")
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::TEMPORARY_REDIRECT);

        let res = TestClient::get("http://127.0.0.1:7878/hello.world")
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
    }
}
