//! Tailing slash middleware

use std::borrow::Cow;
use std::str::FromStr;

use salvo_core::http::response::Body;
use salvo_core::http::uri::{PathAndQuery, Uri};
use salvo_core::prelude::*;

/// TailingSlashAction
#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum TailingSlashAction {
    /// Remove trailing slash.
    Remove,
    /// Add trailing slash.
    Add,
}


/// Default filter used for `TailingSlash` when it's action is [`TailingSlashAction::Add`].
pub fn default_add_filter(req: &Request) -> bool {
    if let Some((_, name)) = req.uri().path().rsplit_once('/') {
        !name.contains('.')
    } else {
        false
    }
}

/// Default filter used for `TailingSlash` when it's action is [`TailingSlashAction::Remove`].
pub fn default_remove_filter(req: &Request) -> bool {
    if let Some((_, name)) = req.uri().path().trim_end_matches('/').rsplit_once('/') {
        name.contains('.')
    } else {
        false
    }
}

/// TailingSlash
pub struct TailingSlash {
    /// Action of this `TailingSlash`.
    pub action: TailingSlashAction,
    /// Remove or add slash only when filter is returns `true`.
    pub filter: Option<Box<dyn Fn(&Request) -> bool + Send + Sync>>,
    /// Redirect code is used when redirect url.
    pub redirect_code: StatusCode,
}
impl TailingSlash {
    /// Create new `TailingSlash`.
    #[inline]
    pub fn new(action: TailingSlashAction) -> Self {
        Self {
            action,
            filter: None,
            redirect_code: StatusCode::MOVED_PERMANENTLY,
        }
    }
    /// Create new `TailingSlash` and sets it's action as [`TailingSlashAction::Add`].
    #[inline]
    pub fn new_add() -> Self {
        Self {
            action: TailingSlashAction::Add,
            filter: None,
            redirect_code: StatusCode::MOVED_PERMANENTLY,
        }
    }
    /// Create new `TailingSlash` and sets it's action as [`TailingSlashAction::Remove`].
    #[inline]
    pub fn new_remove() -> Self {
        Self {
            action: TailingSlashAction::Remove,
            filter: None,
            redirect_code: StatusCode::MOVED_PERMANENTLY,
        }
    }
    /// Set filter and returns new `TailingSlash`.
    #[inline]
    pub fn with_filter(self, filter: impl Fn(&Request) -> bool + Send + Sync + 'static) -> Self {
        Self {
            filter: Some(Box::new(filter)),
            ..self
        }
    }

    /// Set redirect code and returns new `TailingSlash`.
    #[inline]
    pub fn with_redirect_code(self, redirect_code: StatusCode) -> Self {
        Self { redirect_code, ..self }
    }
}

#[async_trait]
impl Handler for TailingSlash {
    #[inline]
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if !self.filter.as_ref().map(|f| f(req)).unwrap_or(true) {
            return;
        }

        let original_path = req.uri().path();
        if !original_path.is_empty() {
            let ends_with_slash = original_path.ends_with('/');
            let new_uri = if self.action == TailingSlashAction::Add && !ends_with_slash {
                Some(replace_uri_path(req.uri(), &format!("{}/", original_path)))
            } else if self.action == TailingSlashAction::Remove && ends_with_slash {
                Some(replace_uri_path(
                    req.uri(),
                    &original_path.trim_end_matches('/').to_string(),
                ))
            } else {
                None
            };
            if let Some(new_uri) = new_uri {
                ctrl.skip_rest();
                res.set_body(Body::None);
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
        Some(query) => Cow::from(format!("{}?{}", new_path, query)),
        None => Cow::from(new_path),
    };
    uri_parts.path_and_query = Some(PathAndQuery::from_str(path.as_ref()).unwrap());
    Uri::from_parts(uri_parts).unwrap()
}

/// Create an add slash middleware.
#[inline]
pub fn add_slash() -> TailingSlash {
    TailingSlash::new(TailingSlashAction::Add).with_filter(default_add_filter)
}

/// Create a remove slash middleware.
#[inline]
pub fn remove_slash() -> TailingSlash {
    TailingSlash::new(TailingSlashAction::Remove).with_filter(default_remove_filter)
}

#[cfg(test)]
mod tests {
    use salvo_core::http::StatusCode;
    use salvo_core::prelude::*;
    use salvo_core::test::TestClient;

    use super::*;

    #[handler]
    async fn hello_world() -> &'static str {
        "Hello World"
    }
    #[tokio::test]
    async fn test_add_slash() {
        let router = Router::with_hoop(add_slash())
            .push(Router::with_path("hello").get(hello_world))
            .push(Router::with_path("hello.world").get(hello_world));
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
            .push(Router::with_path("hello").get(hello_world))
            .push(Router::with_path("hello.world").get(hello_world));
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
