use std::fmt;
use std::sync::Arc;

use super::filter;
use super::{Filter, FnFilter, PathFilter, PathState};
use crate::http::Request;
use crate::Handler;

/// Router struct is used for route request to different handlers.
pub struct Router {
    pub(crate) routers: Vec<Router>,
    pub(crate) filters: Vec<Box<dyn Filter>>,
    pub(crate) hoops: Vec<Arc<dyn Handler>>,
    pub(crate) handler: Option<Arc<dyn Handler>>,
}
#[doc(hidden)]
pub struct DetectMatched {
    pub hoops: Vec<Arc<dyn Handler>>,
    pub handler: Arc<dyn Handler>,
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
            filters: Vec::new(),
            hoops: Vec::new(),
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

    /// Get current router's middlewares reference.
    #[inline]
    pub fn hoops(&self) -> &Vec<Arc<dyn Handler>> {
        &self.hoops
    }
    /// Get current router's middlewares mutable reference.
    #[inline]
    pub fn hoops_mut(&mut self) -> &mut Vec<Arc<dyn Handler>> {
        &mut self.hoops
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
                        hoops: [&self.hoops[..], &dm.hoops[..]].concat(),
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
                    hoops: self.hoops.clone(),
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

    /// Add a handler as middleware, it will run the handler in current router or it's descendants
    /// handle the request.
    #[inline]
    pub fn with_hoop<H: Handler>(handler: H) -> Self {
        Router::new().hoop(handler)
    }

    /// Add a handler as middleware, it will run the handler in current router or it's descendants
    /// handle the request.
    #[inline]
    pub fn hoop<H: Handler>(mut self, handler: H) -> Self {
        self.hoops.push(Arc::new(handler));
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

static SYMBOL_DOWN: &str = "│";
static SYMBOL_TEE: &str = "├";
static SYMBOL_ELL: &str = "└";
static SYMBOL_RIGHT: &str = "─";
impl fmt::Debug for Router {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fn print(f: &mut fmt::Formatter, prefix: &str, last: bool, router: &Router) -> fmt::Result {
            let mut path = "".to_owned();
            let mut others = Vec::with_capacity(router.filters.len());
            if router.filters.is_empty() {
                path = "!NULL!".to_owned();
            } else {
                for filter in &router.filters {
                    let info = format!("{:?}", filter);
                    if info.starts_with("path:") {
                        path = info.split_once(':').unwrap().1.to_owned();
                    } else {
                        let mut parts = info.splitn(2, ':').collect::<Vec<_>>();
                        if !parts.is_empty() {
                            others.push(parts.pop().unwrap().to_owned());
                        }
                    }
                }
            }
            let cp = if last {
                format!("{}{}{}{}", prefix, SYMBOL_ELL, SYMBOL_RIGHT, SYMBOL_RIGHT)
            } else {
                format!("{}{}{}{}", prefix, SYMBOL_TEE, SYMBOL_RIGHT, SYMBOL_RIGHT)
            };
            if !others.is_empty() {
                writeln!(f, "{}{}[{}]", cp, path, others.join(","))?;
            } else {
                writeln!(f, "{}{}", cp, path)?;
            }
            let routers = router.routers();
            if !routers.is_empty() {
                let np = if last {
                    format!("{}    ", prefix)
                } else {
                    format!("{}{}   ", prefix, SYMBOL_DOWN)
                };
                for (i, router) in routers.iter().enumerate() {
                    print(f, &np, i == routers.len() - 1, router)?;
                }
            }
            Ok(())
        }
        print(f, "", true, self)
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
    fn test_router_debug() {
        let router = Router::default()
            .push(
                Router::with_path("users")
                    .push(Router::with_path("<id>").push(Router::with_path("emails").get(fake_handler)))
                    .push(
                        Router::with_path("<id>/articles/<aid>")
                            .get(fake_handler)
                            .delete(fake_handler),
                    ),
            )
            .push(
                Router::with_path("articles")
                    .push(
                        Router::with_path("<id>/authors/<aid>")
                            .get(fake_handler)
                            .delete(fake_handler),
                    )
                    .push(Router::with_path("<id>").get(fake_handler).delete(fake_handler)),
            );
        assert_eq!(
            format!("{:?}", router),
            r#"└──!NULL!
    ├──users
    │   ├──<id>
    │   │   └──emails
    │   │       └──[GET]
    │   └──<id>/articles/<aid>
    │       ├──[GET]
    │       └──[DELETE]
    └──articles
        ├──<id>/authors/<aid>
        │   ├──[GET]
        │   └──[DELETE]
        └──<id>
            ├──[GET]
            └──[DELETE]
"#
        );
    }
    #[test]
    fn test_router_detect1() {
        let router = Router::default().push(
            Router::with_path("users")
                .push(Router::with_path("<id>").push(Router::with_path("emails").get(fake_handler))),
        );
        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/users/12/emails")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect2() {
        let router = Router::new()
            .push(Router::with_path("users").push(Router::with_path("<id>").get(fake_handler)))
            .push(
                Router::with_path("users")
                    .push(Router::with_path("<id>").push(Router::with_path("emails").get(fake_handler))),
            );
        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/users/12/emails")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
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
        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/users/12/facebook/insights")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());

        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/users/12/facebook/insights/23")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
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
        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/users/12/facebook/insights")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        // assert_eq!(format!("{:?}", path_state), "");
        assert!(matched.is_none());

        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/users/12/facebook/insights/23")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect5() {
        let router =
            Router::new().push(Router::with_path("users").push(Router::with_path(r"<id:/\d+/>").push(
                Router::new().push(
                    Router::with_path("facebook/insights").push(Router::with_path("<**rest>").handle(fake_handler)),
                ),
            )));
        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/users/12/facebook/insights")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());

        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/users/12/facebook/insights/23")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
        assert_eq!(path_state.params["id"], "12");
    }
    #[test]
    fn test_router_detect6() {
        let router =
            Router::new().push(Router::with_path("users").push(Router::with_path(r"<id:/\d+/>").push(
                Router::new().push(
                    Router::with_path("facebook/insights").push(Router::new().path("<*rest>").handle(fake_handler)),
                ),
            )));
        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/users/12/facebook/insights")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_none());

        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/users/12/facebook/insights/23")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect_utf8() {
        let router =
            Router::new().push(Router::with_path("用户").push(Router::with_path(r"<id:/\d+/>").push(
                Router::new().push(
                    Router::with_path("facebook/insights").push(Router::with_path("<*rest>").handle(fake_handler)),
                ),
            )));
        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/%E7%94%A8%E6%88%B7/12/facebook/insights")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_none());

        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/%E7%94%A8%E6%88%B7/12/facebook/insights/23")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect9() {
        let router =
            Router::new().push(Router::with_path("users/<*sub:/(images|css)/>/<filename>").handle(fake_handler));
        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/users/12/m.jpg")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_none());

        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/users/css/m.jpg")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect10() {
        let router = Router::new().push(Router::with_path(r"users/<*sub:/(images|css)/.+/>").handle(fake_handler));
        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/users/12/m.jpg")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_none());

        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/users/css/abc/m.jpg")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect11() {
        let router =
            Router::new().push(Router::with_path(r"avatars/<width:/\d+/>x<height:/\d+/>.<ext>").handle(fake_handler));
        // let mut req: Request = hyper::Request::builder()
        //     .uri("http://local.host/avatars/320x320f.webp")
        //     .body(hyper::Body::empty())
        //     .unwrap()
        //     .into();
        // let mut path_state = PathState::new(req.uri().path());
        // let matched = router.detect(&mut req, &mut path_state);
        // assert!(matched.is_none());

        let mut req: Request = hyper::Request::builder()
            .uri("http://local.host/avatars/320x320.webp")
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
}
