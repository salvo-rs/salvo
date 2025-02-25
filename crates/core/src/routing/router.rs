use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::filters::{self, FnFilter, PathFilter};
use super::{DetectMatched, Filter, PathState};
use crate::handler::{Handler, WhenHoop};
use crate::http::uri::Scheme;
use crate::{Depot, Request};

/// Route request to different handlers.
///
/// View [module level documentation](index.html) for more details.
#[non_exhaustive]
pub struct Router {
    #[doc(hidden)]
    pub id: usize,
    /// The children of current router.
    pub routers: Vec<Router>,
    /// The filters of current router.
    pub filters: Vec<Box<dyn Filter>>,
    /// The middlewares of current router.
    pub hoops: Vec<Arc<dyn Handler>>,
    /// The final handler to handle request of current router.
    pub goal: Option<Arc<dyn Handler>>,
}

impl Default for Router {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
static NEXT_ROUTER_ID: AtomicUsize = AtomicUsize::new(1);

impl Router {
    /// Create a new `Router`.
    #[inline]
    pub fn new() -> Self {
        Self {
            id: NEXT_ROUTER_ID.fetch_add(1, Ordering::Relaxed),
            routers: Vec::new(),
            filters: Vec::new(),
            hoops: Vec::new(),
            goal: None,
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
    pub async fn detect(
        &self,
        req: &mut Request,
        path_state: &mut PathState,
    ) -> Option<DetectMatched> {
        Box::pin(async move {
            for filter in &self.filters {
                if !filter.filter(req, path_state).await {
                    return None;
                }
            }
            if !self.routers.is_empty() {
                let original_cursor = path_state.cursor;
                #[cfg(feature = "matched-path")]
                let original_matched_parts_len = path_state.matched_parts.len();
                for child in &self.routers {
                    if let Some(dm) = child.detect(req, path_state).await {
                        return Some(DetectMatched {
                            hoops: [&self.hoops[..], &dm.hoops[..]].concat(),
                            goal: dm.goal.clone(),
                        });
                    } else {
                        #[cfg(feature = "matched-path")]
                        path_state
                            .matched_parts
                            .truncate(original_matched_parts_len);
                        path_state.cursor = original_cursor;
                    }
                }
            }
            if path_state.is_ended() {
                path_state.once_ended = true;
                if let Some(goal) = &self.goal {
                    return Some(DetectMatched {
                        hoops: self.hoops.clone(),
                        goal: goal.clone(),
                    });
                }
            }
            None
        })
        .await
    }

    /// Insert a router at the beginning of current router, shifting all routers after it to the right.
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
    pub fn with_hoop<H: Handler>(hoop: H) -> Self {
        Router::new().hoop(hoop)
    }

    /// Add a handler as middleware, it will run the handler in current router or it's descendants
    /// handle the request. This middleware is only effective when the filter returns true..
    #[inline]
    pub fn with_hoop_when<H, F>(hoop: H, filter: F) -> Self
    where
        H: Handler,
        F: Fn(&Request, &Depot) -> bool + Send + Sync + 'static,
    {
        Router::new().hoop_when(hoop, filter)
    }

    /// Add a handler as middleware, it will run the handler in current router or it's descendants
    /// handle the request.
    #[inline]
    pub fn hoop<H: Handler>(mut self, hoop: H) -> Self {
        self.hoops.push(Arc::new(hoop));
        self
    }

    /// Add a handler as middleware, it will run the handler in current router or it's descendants
    /// handle the request. This middleware is only effective when the filter returns true..
    #[inline]
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
    pub fn goal<H: Handler>(mut self, goal: H) -> Self {
        self.goal = Some(Arc::new(goal));
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

    /// Create a new [`HostFilter`] and set host filter.
    ///
    /// [`HostFilter`]: super::filters::HostFilter
    #[inline]
    pub fn with_host(host: impl Into<String>) -> Self {
        Router::with_filter(filters::host(host))
    }

    /// Add a [`PortFilter`] to current router.
    ///
    /// [`PortFilter`]: super::filters::PortFilter
    #[inline]
    pub fn port(self, port: u16) -> Self {
        self.filter(filters::port(port))
    }

    /// Create a new [`PortFilter`] and set port filter.
    ///
    /// [`PortFilter`]: super::filters::PortFilter
    #[inline]
    pub fn with_port(port: u16) -> Self {
        Router::with_filter(filters::port(port))
    }

    /// reates a new child router with [`MethodFilter`] to filter GET method and set this child router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    pub fn get<H: Handler>(self, goal: H) -> Self {
        self.push(Router::with_filter(filters::get()).goal(goal))
    }

    /// Create a new child router with [`MethodFilter`] to filter post method and set this child router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    pub fn post<H: Handler>(self, goal: H) -> Self {
        self.push(Router::with_filter(filters::post()).goal(goal))
    }

    /// Create a new child router with [`MethodFilter`] to filter put method and set this child router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    pub fn put<H: Handler>(self, goal: H) -> Self {
        self.push(Router::with_filter(filters::put()).goal(goal))
    }

    /// Create a new child router with [`MethodFilter`] to filter delete method and set this child router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    pub fn delete<H: Handler>(self, goal: H) -> Self {
        self.push(Router::with_filter(filters::delete()).goal(goal))
    }

    /// Create a new child router with [`MethodFilter`] to filter patch method and set this child router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    pub fn patch<H: Handler>(self, goal: H) -> Self {
        self.push(Router::with_filter(filters::patch()).goal(goal))
    }

    /// Create a new child router with [`MethodFilter`] to filter head method and set this child router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    pub fn head<H: Handler>(self, goal: H) -> Self {
        self.push(Router::with_filter(filters::head()).goal(goal))
    }

    /// Create a new child router with [`MethodFilter`] to filter options method and set this child router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    pub fn options<H: Handler>(self, goal: H) -> Self {
        self.push(Router::with_filter(filters::options()).goal(goal))
    }
}

const SYMBOL_DOWN: &str = "│";
const SYMBOL_TEE: &str = "├";
const SYMBOL_ELL: &str = "└";
const SYMBOL_RIGHT: &str = "─";
impl Debug for Router {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        fn print(f: &mut Formatter, prefix: &str, last: bool, router: &Router) -> fmt::Result {
            let mut path = "".to_owned();
            let mut others = Vec::with_capacity(router.filters.len());
            if router.filters.is_empty() {
                "!NULL!".clone_into(&mut path);
            } else {
                for filter in &router.filters {
                    let info = format!("{filter:?}");
                    if info.starts_with("path:") {
                        info.split_once(':')
                            .expect("`split_once` get `None`")
                            .1
                            .clone_into(&mut path)
                    } else {
                        let mut parts = info.splitn(2, ':').collect::<Vec<_>>();
                        if !parts.is_empty() {
                            others.push(parts.pop().expect("part should exists.").to_owned());
                        }
                    }
                }
            }
            let cp = if last {
                format!("{prefix}{SYMBOL_ELL}{SYMBOL_RIGHT}{SYMBOL_RIGHT}")
            } else {
                format!("{prefix}{SYMBOL_TEE}{SYMBOL_RIGHT}{SYMBOL_RIGHT}")
            };
            let hd = router
                .goal
                .as_ref()
                .map(|goal| format!(" -> {}", goal.type_name()))
                .unwrap_or_default();
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
    use crate::Response;
    use crate::handler;
    use crate::test::TestClient;

    #[handler]
    async fn fake_handler(_res: &mut Response) {}
    #[test]
    fn test_router_debug() {
        let router = Router::default()
            .push(
                Router::with_path("users")
                    .push(
                        Router::with_path("{id}")
                            .push(Router::with_path("emails").get(fake_handler)),
                    )
                    .push(
                        Router::with_path("{id}/articles/{aid}")
                            .get(fake_handler)
                            .delete(fake_handler),
                    ),
            )
            .push(
                Router::with_path("articles")
                    .push(
                        Router::with_path("{id}/authors/{aid}")
                            .get(fake_handler)
                            .delete(fake_handler),
                    )
                    .push(
                        Router::with_path("{id}")
                            .get(fake_handler)
                            .delete(fake_handler),
                    ),
            );
        assert_eq!(
            format!("{:?}", router),
            r#"└──!NULL!
    ├──users
    │   ├──{id}
    │   │   └──emails
    │   │       └──[GET] -> salvo_core::routing::router::tests::fake_handler
    │   └──{id}/articles/{aid}
    │       ├──[GET] -> salvo_core::routing::router::tests::fake_handler
    │       └──[DELETE] -> salvo_core::routing::router::tests::fake_handler
    └──articles
        ├──{id}/authors/{aid}
        │   ├──[GET] -> salvo_core::routing::router::tests::fake_handler
        │   └──[DELETE] -> salvo_core::routing::router::tests::fake_handler
        └──{id}
            ├──[GET] -> salvo_core::routing::router::tests::fake_handler
            └──[DELETE] -> salvo_core::routing::router::tests::fake_handler
"#
        );
    }
    #[tokio::test]
    async fn test_router_detect1() {
        let router =
            Router::default().push(Router::with_path("users").push(
                Router::with_path("{id}").push(Router::with_path("emails").get(fake_handler)),
            ));
        let mut req = TestClient::get("http://local.host/users/12/emails").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
    }
    #[tokio::test]
    async fn test_router_detect2() {
        let router = Router::new()
            .push(Router::with_path("users").push(Router::with_path("{id}").get(fake_handler)))
            .push(Router::with_path("users").push(
                Router::with_path("{id}").push(Router::with_path("emails").get(fake_handler)),
            ));
        let mut req = TestClient::get("http://local.host/users/12/emails").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
    }
    #[tokio::test]
    async fn test_router_detect3() {
        let router = Router::new().push(
            Router::with_path("users").push(
                Router::with_path(r"{id|\d+}").push(
                    Router::new()
                        .push(Router::with_path("facebook/insights/{**rest}").goal(fake_handler)),
                ),
            ),
        );
        let mut req = TestClient::get("http://local.host/users/12/facebook/insights").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());

        let mut req = TestClient::get("http://local.host/users/12/facebook/insights/23").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        // assert_eq!(format!("{:?}", path_state), "");
        assert!(matched.is_some());
    }
    #[tokio::test]
    async fn test_router_detect4() {
        let router = Router::new().push(
            Router::with_path("users").push(
                Router::with_path(r"{id|\d+}").push(
                    Router::new()
                        .push(Router::with_path("facebook/insights/{*+rest}").goal(fake_handler)),
                ),
            ),
        );
        let mut req = TestClient::get("http://local.host/users/12/facebook/insights").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        // assert_eq!(format!("{:?}", path_state), "");
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/users/12/facebook/insights/23").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
    }
    #[tokio::test]
    async fn test_router_detect5() {
        let router = Router::new().push(
            Router::with_path("users").push(
                Router::with_path(r"{id|\d+}").push(
                    Router::new().push(
                        Router::with_path("facebook/insights")
                            .push(Router::with_path("{**rest}").goal(fake_handler)),
                    ),
                ),
            ),
        );
        let mut req = TestClient::get("http://local.host/users/12/facebook/insights").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());

        let mut req = TestClient::get("http://local.host/users/12/facebook/insights/23").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
        assert_eq!(path_state.params["id"], "12");
    }
    #[tokio::test]
    async fn test_router_detect6() {
        let router = Router::new().push(
            Router::with_path("users").push(
                Router::with_path(r"{id|\d+}").push(
                    Router::new().push(
                        Router::with_path("facebook/insights")
                            .push(Router::new().path("{*+rest}").goal(fake_handler)),
                    ),
                ),
            ),
        );
        let mut req = TestClient::get("http://local.host/users/12/facebook/insights").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/users/12/facebook/insights/23").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
    }
    #[tokio::test]
    async fn test_router_detect_utf8() {
        let router = Router::new().push(
            Router::with_path("用户").push(
                Router::with_path(r"{id|\d+}").push(
                    Router::new().push(
                        Router::with_path("facebook/insights")
                            .push(Router::with_path("{*+rest}").goal(fake_handler)),
                    ),
                ),
            ),
        );
        let mut req =
            TestClient::get("http://local.host/%E7%94%A8%E6%88%B7/12/facebook/insights").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_none());

        let mut req =
            TestClient::get("http://local.host/%E7%94%A8%E6%88%B7/12/facebook/insights/23").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
    }
    #[tokio::test]
    async fn test_router_detect9() {
        let router = Router::new()
            .push(Router::with_path("users/{sub|(images|css)}/{filename}").goal(fake_handler));
        let mut req = TestClient::get("http://local.host/users/12/m.jpg").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/users/css/m.jpg").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
    }
    #[tokio::test]
    async fn test_router_detect10() {
        let router = Router::new()
            .push(Router::with_path(r"users/{*sub|(images|css)/.+}").goal(fake_handler));
        let mut req = TestClient::get("http://local.host/users/12/m.jpg").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/users/css/abc/m.jpg").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
    }
    #[tokio::test]
    async fn test_router_detect11() {
        let router = Router::new()
            .push(Router::with_path(r"avatars/{width|\d+}x{height|\d+}.{ext}").goal(fake_handler));
        let mut req = TestClient::get("http://local.host/avatars/321x641f.webp").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/avatars/320x640.webp").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
    }
    #[tokio::test]
    async fn test_router_detect12() {
        let router = Router::new()
            .push(Router::with_path("/.well-known/acme-challenge/{token}").goal(fake_handler));

        let mut req =
            TestClient::get("http://local.host/.well-known/acme-challenge/q1XXrxIx79uXNl3I")
                .build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
    }

    #[tokio::test]
    async fn test_router_detect13() {
        let router = Router::new()
            .path("user/{id|[0-9a-z]{8}(-[0-9a-z]{4}){3}-[0-9a-z]{12}}")
            .get(fake_handler);
        let mut req =
            TestClient::get("http://local.host/user/726d694c-7af0-4bb0-9d22-706f7e38641e").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
        let mut req =
            TestClient::get("http://local.host/user/726d694c-7af0-4bb0-9d22-706f7e386e").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_none());
    }

    #[tokio::test]
    async fn test_router_detect_path_encoded() {
        let router = Router::new().path("api/{p}").get(fake_handler);
        let mut req = TestClient::get("http://127.0.0.1:6060/api/a%2fb%2fc").build();
        let mut path_state = PathState::new(req.uri().path());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
        assert_eq!(path_state.params["p"], "a/b/c");
    }
}
