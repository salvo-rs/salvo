// use std::fmt::{self, Debug};
use std::sync::Arc;

use super::filter;
use super::{Filter, PathFilter, PathState};
use crate::http::Request;
use crate::Handler;

pub struct Router {
    pub children: Vec<Router>,
    pub filters: Vec<Box<dyn Filter>>,
    pub handler: Option<Arc<dyn Handler>>,
    pub befores: Vec<Arc<dyn Handler>>,
    pub afters: Vec<Arc<dyn Handler>>,
}
pub struct DetectMatched {
    pub handler: Arc<dyn Handler>,
    pub befores: Vec<Arc<dyn Handler>>,
    pub afters: Vec<Arc<dyn Handler>>,
}

// impl Debug for Router {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         write!(f, "{{ : '{}'}}", &self)
//     }
// }

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

impl Router {
    pub fn new() -> Router {
        Router {
            children: Vec::new(),
            befores: Vec::new(),
            afters: Vec::new(),
            filters: Vec::new(),
            handler: None,
        }
    }
    pub fn detect(&self, request: &mut Request, path_state: &mut PathState) -> Option<DetectMatched> {
        for filter in &self.filters {
            if !filter.filter(request, path_state) {
                return None;
            }
        }
        if !self.children.is_empty() {
            let original_cursor = path_state.cursor;
            for child in &self.children {
                if let Some(dm) = child.detect(request, path_state) {
                    return Some(DetectMatched {
                        befores: [&self.befores[..], &dm.befores[..]].concat(),
                        afters: [&dm.afters[..], &self.afters[..]].concat(),
                        handler: dm.handler.clone(),
                    });
                } else {
                    path_state.cursor = original_cursor;
                }
            }
        }
        if let Some(handler) = self.handler.clone() {
            if path_state.ended() {
                return Some(DetectMatched {
                    befores: self.befores.clone(),
                    afters: self.afters.clone(),
                    handler: handler.clone(),
                });
            }
        }
        None
    }

    pub fn visit<F>(self, func: F) -> Self
    where
        F: Fn(Router) -> Router,
    {
        func(self)
    }

    pub fn push(mut self, router: Router) -> Self {
        self.children.push(router);
        self
    }
    pub fn push_when<F>(mut self, func: F) -> Self
    where
        F: Fn(&Router) -> Option<Router>,
    {
        if let Some(router) = func(&self) {
            self.children.push(router);
        }
        self
    }
    pub fn before<H: Handler>(mut self, handler: H) -> Self {
        self.befores.push(Arc::new(handler));
        self
    }
    pub fn after<H: Handler>(mut self, handler: H) -> Self {
        self.afters.push(Arc::new(handler));
        self
    }
    pub fn path(self, path: impl Into<String>) -> Self {
        self.filter(PathFilter::new(path))
    }
    pub fn filter(mut self, filter: impl Filter + Sized) -> Self {
        self.filters.push(Box::new(filter));
        self
    }

    pub fn handle<H: Handler>(mut self, handler: H) -> Self {
        self.handler = Some(Arc::new(handler));
        self
    }
    pub fn handle_when<H, F>(mut self, func: F) -> Self
    where
        H: Handler,
        F: Fn(&Router) -> Option<H>,
    {
        if let Some(handler) = func(&self) {
            self.handler = Some(Arc::new(handler));
        }
        self
    }
    pub fn get<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::get()).handle(handler))
    }
    pub fn get_when<H, F>(self, func: F) -> Self
    where
        H: Handler,
        F: Fn(&Router) -> Option<H>,
    {
        if let Some(handler) = func(&self) {
            self.push(Router::new().filter(filter::get()).handle(handler))
        } else {
            self
        }
    }

    pub fn post<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::post()).handle(handler))
    }
    pub fn post_when<H, F>(self, func: F) -> Self
    where
        H: Handler,
        F: Fn(&Router) -> Option<H>,
    {
        if let Some(handler) = func(&self) {
            self.push(Router::new().filter(filter::post()).handle(handler))
        } else {
            self
        }
    }

    pub fn put<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::put()).handle(handler))
    }
    pub fn put_when<H, F>(self, func: F) -> Self
    where
        H: Handler,
        F: Fn(&Router) -> Option<H>,
    {
        if let Some(handler) = func(&self) {
            self.push(Router::new().filter(filter::put()).handle(handler))
        } else {
            self
        }
    }

    pub fn delete<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::delete()).handle(handler))
    }
    pub fn delete_when<H, F>(self, func: F) -> Self
    where
        H: Handler,
        F: Fn(&Router) -> Option<H>,
    {
        if let Some(handler) = func(&self) {
            self.push(Router::new().filter(filter::delete()).handle(handler))
        } else {
            self
        }
    }

    pub fn head<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::head()).handle(handler))
    }
    pub fn head_when<H, F>(self, func: F) -> Self
    where
        H: Handler,
        F: Fn(&Router) -> Option<H>,
    {
        if let Some(handler) = func(&self) {
            self.push(Router::new().filter(filter::head()).handle(handler))
        } else {
            self
        }
    }

    pub fn patch<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::patch()).handle(handler))
    }
    pub fn patch_when<H, F>(self, func: F) -> Self
    where
        H: Handler,
        F: Fn(&Router) -> Option<H>,
    {
        if let Some(handler) = func(&self) {
            self.push(Router::new().filter(filter::patch()).handle(handler))
        } else {
            self
        }
    }

    pub fn options<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::options()).handle(handler))
    }
    pub fn options_when<H, F>(self, func: F) -> Self
    where
        H: Handler,
        F: Fn(&Router) -> Option<H>,
    {
        if let Some(handler) = func(&self) {
            self.push(Router::new().filter(filter::options()).handle(handler))
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PathState, Router};
    use crate::fn_handler;
    use crate::{Request, Response};

    use async_trait::async_trait;
    #[fn_handler]
    async fn fake_handler(_res: &mut Response) {}
    #[test]
    fn test_router_detect1() {
        let router = Router::new().push(
            Router::new().path("users").push(
                Router::new()
                    .path("<id>")
                    .push(Router::new().path("emails").get(fake_handler)),
            ),
        );
        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/users/12/emails")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect2() {
        let router = Router::new()
            .push(
                Router::new()
                    .path("users")
                    .push(Router::new().path("<id>").get(fake_handler)),
            )
            .push(
                Router::new().path("users").push(
                    Router::new()
                        .path("<id>")
                        .push(Router::new().path("emails").get(fake_handler)),
                ),
            );
        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/users/12/emails")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
}
