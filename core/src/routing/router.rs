// use std::fmt::{self, Debug};
use std::sync::Arc;

use super::filter;
use super::{Filter, FnFilter, PathFilter, PathState};
use crate::http::Request;
use crate::Handler;

pub struct Router {
    pub(crate) routers: Vec<Router>,
    pub(crate) filters: Vec<Box<dyn Filter>>,
    pub(crate) handler: Option<Arc<dyn Handler>>,
    pub(crate) befores: Vec<Arc<dyn Handler>>,
    pub(crate) afters: Vec<Arc<dyn Handler>>,
}
pub struct DetectMatched {
    pub handler: Arc<dyn Handler>,
    pub befores: Vec<Arc<dyn Handler>>,
    pub afters: Vec<Arc<dyn Handler>>,
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

impl Router {
    /// Create a new Router.
    pub fn new() -> Router {
        Router {
            routers: Vec::new(),
            befores: Vec::new(),
            afters: Vec::new(),
            filters: Vec::new(),
            handler: None,
        }
    }

    /// Get current router's children reference.
    #[inline]
    pub fn routers(&self) -> &Vec<Router> {
        &self.routers
    }
    /// Get current router's children mutable reference.
    #[inline]
    pub fn routers_mut(&mut self) -> &mut Vec<Router> {
        &mut self.routers
    }

    /// Get current router's before middlewares reference.
    #[inline]
    pub fn befores(&self) -> &Vec<Arc<dyn Handler>> {
        &self.befores
    }
    /// Get current router's before middlewares mutable reference.
    #[inline]
    pub fn befores_mut(&mut self) -> &mut Vec<Arc<dyn Handler>> {
        &mut self.befores
    }

    /// Get current router's after middlewares reference.
    #[inline]
    pub fn afters(&self) -> &Vec<Arc<dyn Handler>> {
        &self.afters
    }
    /// Get current router's after middlewares mutable reference.
    #[inline]
    pub fn afters_mut(&mut self) -> &mut Vec<Arc<dyn Handler>> {
        &mut self.afters
    }

    /// Get current router's filters reference.
    #[inline]
    pub fn filters(&self) -> &Vec<Box<dyn Filter>> {
        &self.filters
    }
    /// Get current router's filters mutable reference.
    #[inline]
    pub fn filters_mut(&mut self) -> &mut Vec<Box<dyn Filter>> {
        &mut self.filters
    }

    /// Detect current router is matched for current request.
    pub fn detect(&self, req: &mut Request, path_state: &mut PathState) -> Option<DetectMatched> {
        for filter in &self.filters {
            if !filter.filter(req, path_state) {
                return None;
            }
        }
        if !self.routers.is_empty() {
            let original_cursor = path_state.cursor;
            for child in &self.routers {
                if let Some(dm) = child.detect(req, path_state) {
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

    /// Push a router as child of current router.
    #[inline]
    pub fn push(mut self, router: Router) -> Self {
        self.routers.push(router);
        self
    }
    /// Append all routers in a Vec as children of current router.
    #[inline]
    pub fn append(mut self, others: Vec<Router>) -> Self {
        let mut others = others;
        self.routers.append(&mut others);
        self
    }

    #[deprecated(since = "0.10.4", note = "Please use then function instead")]
    #[inline]
    pub fn push_when<F>(mut self, func: F) -> Self
    where
        F: Fn(&Router) -> Option<Router>,
    {
        if let Some(router) = func(&self) {
            self.routers.push(router);
        }
        self
    }

    /// Add a handler as middleware, it will run before the handler in current router or it's descendants
    /// handle the request.
    #[inline]
    pub fn before<H: Handler>(mut self, handler: H) -> Self {
        self.befores.push(Arc::new(handler));
        self
    }

    /// Add a handler as middleware, it will run after the handler in current router or it's descendants
    /// handle the request.
    #[inline]
    pub fn after<H: Handler>(mut self, handler: H) -> Self {
        self.afters.push(Arc::new(handler));
        self
    }

    /// Create a new router and set path filter.
    ///
    /// # Panics
    ///
    /// Panics if path value is not in correct format.
    #[inline]
    pub fn with_path(path: impl Into<String>) -> Self {
        Router::new().filter(PathFilter::new(path))
    }

    /// Create a new path filter for current router.
    ///
    /// # Panics
    ///
    /// Panics if path value is not in correct format.
    #[inline]
    pub fn path(self, path: impl Into<String>) -> Self {
        self.filter(PathFilter::new(path))
    }

    /// Add a filter for current router.
    ///
    #[inline]
    pub fn filter(mut self, filter: impl Filter + Sized) -> Self {
        self.filters.push(Box::new(filter));
        self
    }

    /// Create a new FnFilter from Fn.
    #[inline]
    pub fn filter_fn<T>(mut self, func: T) -> Self
    where
        T: Fn(&mut Request, &mut PathState) -> bool + Send + Sync + 'static,
    {
        self.filters.push(Box::new(FnFilter(func)));
        self
    }

    /// Set current router's handler.
    #[inline]
    pub fn handle<H: Handler>(mut self, handler: H) -> Self {
        self.handler = Some(Arc::new(handler));
        self
    }

    /// When you want write router chain, this function will be useful,
    /// You can write your custom logic in FnOnce.
    ///
    #[inline]
    pub fn then<F>(self, func: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        func(self)
    }

    /// Create a new child router with MethodFilter to filter get method and set this child router's handler.
    #[inline]
    pub fn get<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::get()).handle(handler))
    }

    /// Create a new child router with MethodFilter to filter post method and set this child router's handler.
    #[inline]
    pub fn post<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::post()).handle(handler))
    }

    /// Create a new child router with MethodFilter to filter put method and set this child router's handler.
    #[inline]
    pub fn put<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::put()).handle(handler))
    }

    /// Create a new child router with MethodFilter to filter delete method and set this child router's handler.
    #[inline]
    pub fn delete<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::delete()).handle(handler))
    }

    /// Create a new child router with MethodFilter to filter patch method and set this child router's handler.
    #[inline]
    pub fn patch<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::patch()).handle(handler))
    }

    /// Create a new child router with MethodFilter to filter head method and set this child router's handler.
    #[inline]
    pub fn head<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::head()).handle(handler))
    }

    /// Create a new child router with MethodFilter to filter options method and set this child router's handler.
    #[inline]
    pub fn options<H: Handler>(self, handler: H) -> Self {
        self.push(Router::new().filter(filter::options()).handle(handler))
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
            Router::with_path("users").push(
                Router::with_path("<id>")
                    .push(Router::with_path("emails").get(fake_handler)),
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
                Router::with_path("users")
                    .push(Router::with_path("<id>").get(fake_handler)),
            )
            .push(
                Router::with_path("users").push(
                    Router::with_path("<id>")
                        .push(Router::with_path("emails").get(fake_handler)),
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
    fn test_router_detect3() {
        let router = Router::new().push(
            Router::with_path("users").push(
                Router::with_path(r"<id:/\d+/>")
                    .push(Router::new().push(Router::with_path("facebook/insights/<**rest>").handle(fake_handler))),
            ),
        );
        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/users/12/facebook/insights")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());

        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/users/12/facebook/insights/23")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        // assert_eq!(format!("{:?}", path_state), "");
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect4() {
        let router = Router::new().push(
            Router::with_path("users").push(
                Router::with_path(r"<id:/\d+/>")
                    .push(Router::new().push(Router::with_path("facebook/insights/<*rest>").handle(fake_handler))),
            ),
        );
        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/users/12/facebook/insights")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        // assert_eq!(format!("{:?}", path_state), "");
        assert!(matched.is_none());

        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/users/12/facebook/insights/23")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect5() {
        let router = Router::new().push(
            Router::with_path("users").push(
                Router::with_path(r"<id:/\d+/>").push(
                    Router::new().push(
                        Router::with_path("facebook/insights")
                            .push(Router::with_path("<**rest>").handle(fake_handler)),
                    ),
                ),
            ),
        );
        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/users/12/facebook/insights")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());

        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/users/12/facebook/insights/23")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect6() {
        let router = Router::new().push(
            Router::with_path("users").push(
                Router::with_path(r"<id:/\d+/>").push(
                    Router::new().push(
                        Router::with_path("facebook/insights")
                            .push(Router::with_path("<*rest>").handle(fake_handler)),
                    ),
                ),
            ),
        );
        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/users/12/facebook/insights")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_none());

        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/users/12/facebook/insights/23")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect_utf8() {
        let router = Router::new().push(
            Router::with_path("用户").push(
                Router::with_path(r"<id:/\d+/>").push(
                    Router::new().push(
                        Router::with_path("facebook/insights")
                            .push(Router::with_path("<*rest>").handle(fake_handler)),
                    ),
                ),
            ),
        );
        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/%E7%94%A8%E6%88%B7/12/facebook/insights")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_none());

        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/%E7%94%A8%E6%88%B7/12/facebook/insights/23")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect9() {
        let router = Router::new().push(
            Router::with_path("users/<*sub:/(images|css)/>/<filename>")
                .handle(fake_handler),
        );
        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/users/12/m.jpg")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_none());

        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/users/css/m.jpg")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect10() {
        let router = Router::new().push(
            Router::with_path(r"users/<*sub:/(images|css)/.+/>")
                .handle(fake_handler),
        );
        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/users/12/m.jpg")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_none());

        let mut req = Request::from_hyper(
            hyper::Request::builder()
                .uri("http://local.host/users/css/abc/m.jpg")
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
}
