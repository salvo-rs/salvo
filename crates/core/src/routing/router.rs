use std::fmt::{self, Formatter};
use std::sync::Arc;

use super::filters;
use super::{Filter, FnFilter, PathFilter, PathState};
use crate::handler::{Handler, WhenHoop};
use crate::http::uri::Scheme;
use crate::{Depot, Request};

/// Router struct is used for route request to different handlers.
///
/// You can write routers in flat way, like this:
///
/// # Example
///
/// ```
/// # use salvo_core::prelude::*;
///
/// # #[handler]
/// # async fn create_writer(res: &mut Response) {
/// # }
/// # #[handler]
/// # async fn show_writer(res: &mut Response) {
/// # }
/// # #[handler]
/// # async fn list_writers(res: &mut Response) {
/// # }
/// # #[handler]
/// # async fn edit_writer(res: &mut Response) {
/// # }
/// # #[handler]
/// # async fn delete_writer(res: &mut Response) {
/// # }
/// # #[handler]
/// # async fn list_writer_articles(res: &mut Response) {
/// # }
/// # #[tokio::main]
/// # async fn main() {
/// Router::with_path("writers").get(list_writers).post(create_writer);
/// Router::with_path("writers/<id>").get(show_writer).patch(edit_writer).delete(delete_writer);
/// Router::with_path("writers/<id>/articles").get(list_writer_articles);
/// # }
/// ```
///
/// You can write router like a tree, this is also the recommended way:
///
/// # Example
///
/// ```
/// use salvo_core::prelude::*;
///
/// # #[handler]
/// # async fn create_writer(res: &mut Response) {
/// # }
/// # #[handler]
/// # async fn show_writer(res: &mut Response) {
/// # }
/// # #[handler]
/// # async fn list_writers(res: &mut Response) {
/// # }
/// # #[handler]
/// # async fn edit_writer(res: &mut Response) {
/// # }
/// # #[handler]
/// # async fn delete_writer(res: &mut Response) {
/// # }
/// # #[handler]
/// # async fn list_writer_articles(res: &mut Response) {
/// # }
/// # #[tokio::main]
/// # async fn main() {
/// Router::with_path("writers")
///     .get(list_writers)
///     .post(create_writer)
///     .push(
///         Router::with_path("<id>")
///             .get(show_writer)
///             .patch(edit_writer)
///             .delete(delete_writer)
///             .push(Router::with_path("articles").get(list_writer_articles)),
///     );
/// # }
/// ```
///
/// This form of definition can make the definition of router clear and simple for complex projects.
#[non_exhaustive]
pub struct Router {
    /// routers is the children of current router.
    pub routers: Vec<Router>,
    /// filters is the filters of current router.
    pub filters: Vec<Box<dyn Filter>>,
    /// hoops is the middlewares of current router.
    pub hoops: Vec<Arc<dyn Handler>>,
    /// handler is the handler of current router.
    pub handler: Option<Arc<dyn Handler>>,
}
#[doc(hidden)]
pub struct DetectMatched {
    pub hoops: Vec<Arc<dyn Handler>>,
    pub handler: Arc<dyn Handler>,
}

impl Default for Router {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Router {
    /// Create a new `Router`.
    #[inline]
    pub fn new() -> Self {
        Self {
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
                    handler,
                });
            }
        }
        None
    }

    /// Insert a router at the begining of current router, shifting all routers after it to the right.
    #[inline]
    pub fn unshift(mut self, router: Router) -> Self {
        self.routers.insert(0, router);
        self
    }
    /// Insert a router at position `index` within current router, shifting all routers after it to the right.
    #[inline]
    pub fn insert(mut self, index: usize, router: Router) -> Self {
        self.routers.insert(index, router);
        self
    }

    /// Push a router as child of current router.
    #[inline]
    pub fn push(mut self, router: Router) -> Self {
        self.routers.push(router);
        self
    }
    /// Append all routers in a Vec as children of current router.
    #[inline]
    pub fn append(mut self, others: &mut Vec<Router>) -> Self {
        self.routers.append(others);
        self
    }

    /// Add a handler as middleware, it will run the handler in current router or it's descendants
    /// handle the request.
    #[inline]
    pub fn with_hoop<H: Handler>(handler: H) -> Self {
        Router::new().hoop(handler)
    }

    /// Add a handler as middleware, it will run the handler in current router or it's descendants
    /// handle the request. This middleware only effective when the filter return true.
    #[inline]
    pub fn with_hoop_when<H, F>(handler: H, filter: F) -> Self
    where
        H: Handler,
        F: Fn(&Request, &Depot) -> bool + Send + Sync + 'static,
    {
        Router::new().hoop_when(handler, filter)
    }

    /// Add a handler as middleware, it will run the handler in current router or it's descendants
    /// handle the request.
    #[inline]
    pub fn hoop<H: Handler>(mut self, handler: H) -> Self {
        self.hoops.push(Arc::new(handler));
        self
    }

    /// Add a handler as middleware, it will run the handler in current router or it's descendants
    /// handle the request. This middleware only effective when the filter return true.
    #[inline]
    pub fn hoop_when<H, F>(mut self, handler: H, filter: F) -> Self
    where
        H: Handler,
        F: Fn(&Request, &Depot) -> bool + Send + Sync + 'static,
    {
        self.hoops.push(Arc::new(WhenHoop { inner: handler, filter }));
        self
    }

    /// Create a new router and set path filter.
    ///
    /// # Panics
    ///
    /// Panics if path value is not in correct format.
    #[inline]
    pub fn with_path(path: impl Into<String>) -> Self {
        Router::with_filter(PathFilter::new(path))
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

    /// Create a new router and set filter.
    #[inline]
    pub fn with_filter(filter: impl Filter + Sized) -> Self {
        Router::new().filter(filter)
    }
    /// Add a filter for current router.
    #[inline]
    pub fn filter(mut self, filter: impl Filter + Sized) -> Self {
        self.filters.push(Box::new(filter));
        self
    }

    /// Create a new router and set filter_fn.
    #[inline]
    pub fn with_filter_fn<T>(func: T) -> Self
    where
        T: Fn(&mut Request, &mut PathState) -> bool + Send + Sync + 'static,
    {
        Router::with_filter(FnFilter(func))
    }
    /// Create a new FnFilter from Fn.
    #[inline]
    pub fn filter_fn<T>(self, func: T) -> Self
    where
        T: Fn(&mut Request, &mut PathState) -> bool + Send + Sync + 'static,
    {
        self.filter(FnFilter(func))
    }

    /// Sets current router's handler.
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

    /// Add a [`SchemeFilter`] to current router.
    ///
    /// [`SchemeFilter`]: super::filters::HostFilter
    #[inline]
    pub fn scheme(self, scheme: Scheme) -> Self {
        self.filter(filters::scheme(scheme))
    }

    /// Add a [`HostFilter`] to current router.
    ///
    /// [`HostFilter`]: super::filters::HostFilter
    #[inline]
    pub fn host(self, host: impl Into<String>) -> Self {
        self.filter(filters::host(host))
    }

    /// Add a [`PortFilter`] to current router.
    ///
    /// [`PortFilter`]: super::filters::PortFilter
    #[inline]
    pub fn port(self, port: u16) -> Self {
        self.filter(filters::port(port))
    }

    /// Create a new child router with [`MethodFilter`] to filter get method and set this child router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    pub fn get<H: Handler>(self, handler: H) -> Self {
        self.push(Router::with_filter(filters::get()).handle(handler))
    }

    /// Create a new child router with [`MethodFilter`] to filter post method and set this child router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    pub fn post<H: Handler>(self, handler: H) -> Self {
        self.push(Router::with_filter(filters::post()).handle(handler))
    }

    /// Create a new child router with [`MethodFilter`] to filter put method and set this child router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    pub fn put<H: Handler>(self, handler: H) -> Self {
        self.push(Router::with_filter(filters::put()).handle(handler))
    }

    /// Create a new child router with [`MethodFilter`] to filter delete method and set this child router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    pub fn delete<H: Handler>(self, handler: H) -> Self {
        self.push(Router::with_filter(filters::delete()).handle(handler))
    }

    /// Create a new child router with [`MethodFilter`] to filter patch method and set this child router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    pub fn patch<H: Handler>(self, handler: H) -> Self {
        self.push(Router::with_filter(filters::patch()).handle(handler))
    }

    /// Create a new child router with [`MethodFilter`] to filter head method and set this child router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    pub fn head<H: Handler>(self, handler: H) -> Self {
        self.push(Router::with_filter(filters::head()).handle(handler))
    }

    /// Create a new child router with [`MethodFilter`] to filter options method and set this child router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    pub fn options<H: Handler>(self, handler: H) -> Self {
        self.push(Router::with_filter(filters::options()).handle(handler))
    }
}

const SYMBOL_DOWN: &str = "│";
const SYMBOL_TEE: &str = "├";
const SYMBOL_ELL: &str = "└";
const SYMBOL_RIGHT: &str = "─";
impl fmt::Debug for Router {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        fn print(f: &mut Formatter, prefix: &str, last: bool, router: &Router) -> fmt::Result {
            let mut path = "".to_owned();
            let mut others = Vec::with_capacity(router.filters.len());
            if router.filters.is_empty() {
                path = "!NULL!".to_owned();
            } else {
                for filter in &router.filters {
                    let info = format!("{filter:?}");
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
                format!("{prefix}{SYMBOL_ELL}{SYMBOL_RIGHT}{SYMBOL_RIGHT}")
            } else {
                format!("{prefix}{SYMBOL_TEE}{SYMBOL_RIGHT}{SYMBOL_RIGHT}")
            };
            let hd = if let Some(handler) = &router.handler {
                format!(" -> {}", handler.type_name())
            } else {
                "".into()
            };
            if !others.is_empty() {
                writeln!(f, "{cp}{path}[{}]{hd}", others.join(","))?;
            } else {
                writeln!(f, "{cp}{path}{hd}")?;
            }
            let routers = router.routers();
            if !routers.is_empty() {
                let np = if last {
                    format!("{prefix}    ")
                } else {
                    format!("{prefix}{SYMBOL_DOWN}   ")
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
    use crate::handler;
    use crate::test::TestClient;
    use crate::Response;

    #[handler]
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
    │   │       └──[GET] -> salvo_core::routing::router::tests::fake_handler
    │   └──<id>/articles/<aid>
    │       ├──[GET] -> salvo_core::routing::router::tests::fake_handler
    │       └──[DELETE] -> salvo_core::routing::router::tests::fake_handler
    └──articles
        ├──<id>/authors/<aid>
        │   ├──[GET] -> salvo_core::routing::router::tests::fake_handler
        │   └──[DELETE] -> salvo_core::routing::router::tests::fake_handler
        └──<id>
            ├──[GET] -> salvo_core::routing::router::tests::fake_handler
            └──[DELETE] -> salvo_core::routing::router::tests::fake_handler
"#
        );
    }
    #[test]
    fn test_router_detect1() {
        let router = Router::default().push(
            Router::with_path("users")
                .push(Router::with_path("<id>").push(Router::with_path("emails").get(fake_handler))),
        );
        let mut req = TestClient::get("http://local.host/users/12/emails").build();
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
        let mut req = TestClient::get("http://local.host/users/12/emails").build();
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
        let mut req = TestClient::get("http://local.host/users/12/facebook/insights").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());

        let mut req = TestClient::get("http://local.host/users/12/facebook/insights/23").build();
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
        let mut req = TestClient::get("http://local.host/users/12/facebook/insights").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        // assert_eq!(format!("{:?}", path_state), "");
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/users/12/facebook/insights/23").build();
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
        let mut req = TestClient::get("http://local.host/users/12/facebook/insights").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());

        let mut req = TestClient::get("http://local.host/users/12/facebook/insights/23").build();
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
        let mut req = TestClient::get("http://local.host/users/12/facebook/insights").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/users/12/facebook/insights/23").build();
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
        let mut req = TestClient::get("http://local.host/%E7%94%A8%E6%88%B7/12/facebook/insights").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/%E7%94%A8%E6%88%B7/12/facebook/insights/23").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect9() {
        let router =
            Router::new().push(Router::with_path("users/<sub:/(images|css)/>/<filename>").handle(fake_handler));
        let mut req = TestClient::get("http://local.host/users/12/m.jpg").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/users/css/m.jpg").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect10() {
        let router = Router::new().push(Router::with_path(r"users/<*sub:/(images|css)/.+/>").handle(fake_handler));
        let mut req = TestClient::get("http://local.host/users/12/m.jpg").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/users/css/abc/m.jpg").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect11() {
        let router =
            Router::new().push(Router::with_path(r"avatars/<width:/\d+/>x<height:/\d+/>.<ext>").handle(fake_handler));
        let mut req = TestClient::get("http://local.host/avatars/321x641f.webp").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/avatars/320x640.webp").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }
    #[test]
    fn test_router_detect12() {
        let router = Router::new().push(Router::with_path("/.well-known/acme-challenge/<token>").handle(fake_handler));

        let mut req = TestClient::get("http://local.host/.well-known/acme-challenge/q1XXrxIx79uXNl3I").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
    }

    #[test]
    fn test_router_detect13() {
        let router = Router::new()
            .path("user/<id:/[0-9a-z]{8}(-[0-9a-z]{4}){3}-[0-9a-z]{12}/>")
            .get(fake_handler);
        let mut req = TestClient::get("http://local.host/user/726d694c-7af0-4bb0-9d22-706f7e38641e").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
        let mut req = TestClient::get("http://local.host/user/726d694c-7af0-4bb0-9d22-706f7e386e").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_none());
    }

    #[test]
    fn test_router_detect_path_encoded() {
        let router = Router::new().path("api/<p>").get(fake_handler);
        let mut req = TestClient::get("http://127.0.0.1:6060/api/a%2fb%2fc").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state);
        assert!(matched.is_some());
        assert_eq!(path_state.params["p"], "a/b/c");
    }
}
