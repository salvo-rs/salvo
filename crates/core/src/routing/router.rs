use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::filters::{self, FnFilter, PathFilter};
use super::{DetectMatched, Filter, FilterInfo, PathState};
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
    pub routers: Vec<Self>,
    /// The filters of current router.
    pub filters: Vec<Box<dyn Filter>>,
    /// The middlewares of current router.
    pub hoops: Vec<Arc<dyn Handler>>,
    /// The final handler to handle request of current router.
    pub goal: Option<Arc<dyn Handler>>,
}

struct DetectFrame<'a> {
    router: &'a Router,
    next_child: usize,
    original_cursor: (usize, usize),
    params_snapshot: (usize, bool, usize),
    #[cfg(feature = "matched-path")]
    original_matched_parts_len: usize,
}

impl<'a> DetectFrame<'a> {
    fn new(router: &'a Router, path_state: &PathState) -> Self {
        Self {
            router,
            next_child: 0,
            original_cursor: path_state.cursor,
            params_snapshot: path_state.params.snapshot(),
            #[cfg(feature = "matched-path")]
            original_matched_parts_len: path_state.matched_parts.len(),
        }
    }

    fn rollback(&self, path_state: &mut PathState) {
        #[cfg(feature = "matched-path")]
        path_state
            .matched_parts
            .truncate(self.original_matched_parts_len);
        path_state.cursor = self.original_cursor;
        path_state.params.rollback(self.params_snapshot);
    }
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
    #[must_use]
    pub fn routers(&self) -> &Vec<Self> {
        &self.routers
    }
    /// Get current router's children mutable reference.
    #[inline]
    pub fn routers_mut(&mut self) -> &mut Vec<Self> {
        &mut self.routers
    }

    /// Get current router's middlewares reference.
    #[inline]
    #[must_use]
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
    #[must_use]
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
        path_state: &mut PathState<'_>,
    ) -> Option<DetectMatched> {
        if !self.filters_match(req, path_state).await {
            return None;
        }

        let mut stack = vec![DetectFrame::new(self, path_state)];
        loop {
            let child = {
                let frame = stack
                    .last_mut()
                    .expect("detect stack always contains the current router");
                if frame.next_child < frame.router.routers.len() {
                    let child = &frame.router.routers[frame.next_child];
                    frame.next_child += 1;
                    Some(child)
                } else {
                    None
                }
            };

            if let Some(child) = child {
                if child.filters_match(req, path_state).await {
                    stack.push(DetectFrame::new(child, path_state));
                } else {
                    stack
                        .last()
                        .expect("parent frame exists while testing a child")
                        .rollback(path_state);
                }
                continue;
            }

            let frame = stack
                .last()
                .expect("detect stack always contains the current router");
            if path_state.is_ended() {
                path_state.once_ended = true;
                if let Some(goal) = &frame.router.goal {
                    return Some(Self::matched_from_stack(&stack, goal));
                }
            }

            stack.pop();
            if let Some(parent) = stack.last() {
                parent.rollback(path_state);
            } else {
                return None;
            }
        }
    }

    async fn filters_match(&self, req: &mut Request, path_state: &mut PathState<'_>) -> bool {
        for filter in &self.filters {
            if !filter.filter(req, path_state).await {
                return false;
            }
        }
        true
    }

    fn matched_from_stack(stack: &[DetectFrame<'_>], goal: &Arc<dyn Handler>) -> DetectMatched {
        let hoops_len = stack.iter().map(|frame| frame.router.hoops.len()).sum();
        let mut hoops = Vec::with_capacity(hoops_len);
        for frame in stack {
            hoops.extend_from_slice(&frame.router.hoops);
        }
        DetectMatched {
            hoops,
            goal: goal.clone(),
        }
    }

    /// Insert a router at the beginning of current router, shifting all routers after it to the
    /// right.
    #[inline]
    #[must_use]
    pub fn unshift(mut self, router: Self) -> Self {
        self.routers.insert(0, router);
        self
    }
    /// Insert a router at position `index` within current router, shifting all routers after it to
    /// the right.
    #[inline]
    #[must_use]
    pub fn insert(mut self, index: usize, router: Self) -> Self {
        self.routers.insert(index, router);
        self
    }

    /// Push a router as child of current router.
    #[inline]
    #[must_use]
    pub fn push(mut self, router: Self) -> Self {
        self.routers.push(router);
        self
    }
    /// Append all routers in a Vec as children of current router.
    #[inline]
    #[must_use]
    pub fn append(mut self, others: &mut Vec<Self>) -> Self {
        self.routers.append(others);
        self
    }

    /// Add a handler as middleware. It runs the handler in the current router or its
    /// descendants when handling the request.
    #[inline]
    #[must_use]
    pub fn with_hoop<H: Handler>(hoop: H) -> Self {
        Self::new().hoop(hoop)
    }

    /// Add a handler as middleware. It runs the handler in the current router or its
    /// descendants when handling the request, but only when the filter returns `true`.
    #[inline]
    #[must_use]
    pub fn with_hoop_when<H, F>(hoop: H, filter: F) -> Self
    where
        H: Handler,
        F: Fn(&Request, &Depot) -> bool + Send + Sync + 'static,
    {
        Self::new().hoop_when(hoop, filter)
    }

    /// Add a handler as middleware. It runs the handler in the current router or its
    /// descendants when handling the request.
    #[inline]
    #[must_use]
    pub fn hoop<H: Handler>(mut self, hoop: H) -> Self {
        self.hoops.push(Arc::new(hoop));
        self
    }

    /// Add a handler as middleware. It runs the handler in the current router or its
    /// descendants when handling the request, but only when the filter returns `true`.
    #[inline]
    #[must_use]
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
    /// Invalid path patterns are logged and converted into a filter that never matches.
    /// Use [`Router::try_with_path`] to handle malformed patterns explicitly.
    #[inline]
    #[must_use]
    pub fn with_path(path: impl Into<String>) -> Self {
        Self::with_filter(PathFilter::new(path))
    }

    /// Try creating a new router and set path filter.
    ///
    /// # Errors
    ///
    /// Returns an error when the path pattern is malformed.
    #[inline]
    pub fn try_with_path(path: impl Into<String>) -> Result<Self, String> {
        Ok(Self::with_filter(PathFilter::try_new(path)?))
    }

    /// Create a new path filter for current router.
    ///
    /// Invalid path patterns are logged and converted into a filter that never matches.
    /// Use [`Router::try_path`] to handle malformed patterns explicitly.
    #[inline]
    #[must_use]
    pub fn path(self, path: impl Into<String>) -> Self {
        self.filter(PathFilter::new(path))
    }

    /// Try creating a new path filter for current router.
    ///
    /// # Errors
    ///
    /// Returns an error when the path pattern is malformed.
    #[inline]
    pub fn try_path(self, path: impl Into<String>) -> Result<Self, String> {
        Ok(self.filter(PathFilter::try_new(path)?))
    }

    /// Create a new router and set filter.
    #[inline]
    #[must_use]
    pub fn with_filter(filter: impl Filter + Sized) -> Self {
        Self::new().filter(filter)
    }
    /// Add a filter for current router.
    #[inline]
    #[must_use]
    pub fn filter(mut self, filter: impl Filter + Sized) -> Self {
        self.filters.push(Box::new(filter));
        self
    }

    /// Create a new router and set filter_fn.
    #[inline]
    #[must_use]
    pub fn with_filter_fn<T>(func: T) -> Self
    where
        T: for<'a> Fn(&mut Request, &mut PathState<'a>) -> bool + Send + Sync + 'static,
    {
        Self::with_filter(FnFilter(func))
    }
    /// Create a new FnFilter from Fn.
    #[inline]
    #[must_use]
    pub fn filter_fn<T>(self, func: T) -> Self
    where
        T: for<'a> Fn(&mut Request, &mut PathState<'a>) -> bool + Send + Sync + 'static,
    {
        self.filter(FnFilter(func))
    }

    /// Sets current router's handler.
    ///
    /// Calling `goal` on a router that already has one **replaces** the previous
    /// handler (a warning is logged, since this usually indicates route-building
    /// code overwriting itself by accident). Note that the method helpers
    /// ([`get`](Router::get), [`post`](Router::post), ...) do not set this
    /// router's goal — each pushes a filtered child router with its own goal.
    #[inline]
    #[must_use]
    pub fn goal<H: Handler>(mut self, goal: H) -> Self {
        if self.goal.is_some() {
            tracing::warn!(
                "`Router::goal` called on a router that already has a goal handler; \
                 the previous handler is replaced. If you meant to serve multiple \
                 methods or paths, push child routers instead."
            );
        }
        self.goal = Some(Arc::new(goal));
        self
    }

    /// Runs a closure with this router and returns the closure result.
    /// Useful for composing router chains conditionally.
    #[inline]
    #[must_use]
    pub fn then<F>(self, func: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        func(self)
    }

    /// Add a [`SchemeFilter`] to current router.
    ///
    /// [`SchemeFilter`]: super::filters::SchemeFilter
    #[inline]
    #[must_use]
    pub fn scheme(self, scheme: Scheme) -> Self {
        self.filter(filters::scheme(scheme))
    }

    /// Add a [`HostFilter`] to current router.
    ///
    /// [`HostFilter`]: super::filters::HostFilter
    #[inline]
    #[must_use]
    pub fn host(self, host: impl Into<String>) -> Self {
        self.filter(filters::host(host))
    }

    /// Create a new [`HostFilter`] and set host filter.
    ///
    /// [`HostFilter`]: super::filters::HostFilter
    #[inline]
    #[must_use]
    pub fn with_host(host: impl Into<String>) -> Self {
        Self::with_filter(filters::host(host))
    }

    /// Add a [`PortFilter`] to current router.
    ///
    /// [`PortFilter`]: super::filters::PortFilter
    #[inline]
    #[must_use]
    pub fn port(self, port: u16) -> Self {
        self.filter(filters::port(port))
    }

    /// Create a new [`PortFilter`] and set port filter.
    ///
    /// [`PortFilter`]: super::filters::PortFilter
    #[inline]
    #[must_use]
    pub fn with_port(port: u16) -> Self {
        Self::with_filter(filters::port(port))
    }

    /// Creates a new child router with [`MethodFilter`] to filter GET method and set this child
    /// router's handler.
    ///
    /// Each method helper (`get`, `post`, ...) pushes a filtered **child** router;
    /// it does not set the goal of `self`. Two consequences worth knowing:
    ///
    /// - `Router::with_path("x").get(a).post(b)` builds one path router with two method children —
    ///   the usual pattern.
    /// - `.get(a).get(b)` registers two **sibling** GET routes rather than replacing `a`; the first
    ///   match wins, so `b` is unreachable.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    #[must_use]
    pub fn get<H: Handler>(self, goal: H) -> Self {
        self.push(Self::with_filter(filters::get()).goal(goal))
    }

    /// Create a new child router with [`MethodFilter`] to filter post method and set this child
    /// router's handler.
    ///
    /// See [`get`](Router::get) for how method helpers compose as child routers.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    #[must_use]
    pub fn post<H: Handler>(self, goal: H) -> Self {
        self.push(Self::with_filter(filters::post()).goal(goal))
    }

    /// Create a new child router with [`MethodFilter`] to filter put method and set this child
    /// router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    #[must_use]
    pub fn put<H: Handler>(self, goal: H) -> Self {
        self.push(Self::with_filter(filters::put()).goal(goal))
    }

    /// Create a new child router with [`MethodFilter`] to filter delete method and set this child
    /// router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    #[must_use]
    pub fn delete<H: Handler>(self, goal: H) -> Self {
        self.push(Self::with_filter(filters::delete()).goal(goal))
    }

    /// Create a new child router with [`MethodFilter`] to filter patch method and set this child
    /// router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    #[must_use]
    pub fn patch<H: Handler>(self, goal: H) -> Self {
        self.push(Self::with_filter(filters::patch()).goal(goal))
    }

    /// Create a new child router with [`MethodFilter`] to filter head method and set this child
    /// router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    #[must_use]
    pub fn head<H: Handler>(self, goal: H) -> Self {
        self.push(Self::with_filter(filters::head()).goal(goal))
    }

    /// Create a new child router with [`MethodFilter`] to filter options method and set this child
    /// router's handler.
    ///
    /// [`MethodFilter`]: super::filters::MethodFilter
    #[inline]
    #[must_use]
    pub fn options<H: Handler>(self, goal: H) -> Self {
        self.push(Self::with_filter(filters::options()).goal(goal))
    }
}

const SYMBOL_DOWN: &str = "│";
const SYMBOL_TEE: &str = "├";
const SYMBOL_ELL: &str = "└";
const SYMBOL_RIGHT: &str = "─";
impl Debug for Router {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        fn print(f: &mut Formatter, prefix: &str, last: bool, router: &Router) -> fmt::Result {
            let mut path = String::new();
            let mut others = Vec::with_capacity(router.filters.len());
            if router.filters.is_empty() {
                "!NULL!".clone_into(&mut path);
            } else {
                for filter in &router.filters {
                    match filter.info() {
                        FilterInfo::Path(p) => path = p,
                        FilterInfo::Method(m) => others.push(format!("{m:?}")),
                        FilterInfo::Scheme(s) => others.push(format!("{s:?}")),
                        FilterInfo::Host(h) => others.push(format!("{h:?}")),
                        FilterInfo::Port(p) => others.push(format!("{p:?}")),
                        FilterInfo::Other(_) => others.push(format!("{filter:?}")),
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
    use crate::test::TestClient;
    use crate::{Response, handler};

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
            format!("{router:?}"),
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
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
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
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
    }
    #[tokio::test]
    async fn test_router_detect_parent_goal_after_child_mismatch() {
        let router = Router::with_path("users")
            .get(fake_handler)
            .push(Router::with_path("{id}/profile").get(fake_handler));
        let mut req = TestClient::get("http://local.host/users").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
        assert!(path_state.params.is_empty());
        assert!(path_state.is_ended());
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
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());

        let mut req = TestClient::get("http://local.host/users/12/facebook/insights/23").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
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
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        // assert_eq!(format!("{:?}", path_state), "");
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/users/12/facebook/insights/23").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
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
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());

        let mut req = TestClient::get("http://local.host/users/12/facebook/insights/23").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
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
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/users/12/facebook/insights/23").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
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
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_none());

        let mut req =
            TestClient::get("http://local.host/%E7%94%A8%E6%88%B7/12/facebook/insights/23").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
    }
    #[tokio::test]
    async fn test_router_detect9() {
        let router = Router::new()
            .push(Router::with_path("users/{sub|(images|css)}/{filename}").goal(fake_handler));
        let mut req = TestClient::get("http://local.host/users/12/m.jpg").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/users/css/m.jpg").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
    }
    #[tokio::test]
    async fn test_router_detect10() {
        let router = Router::new()
            .push(Router::with_path(r"users/{*sub|(images|css)/.+}").goal(fake_handler));
        let mut req = TestClient::get("http://local.host/users/12/m.jpg").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/users/css/abc/m.jpg").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
    }
    #[tokio::test]
    async fn test_router_detect11() {
        let router = Router::new()
            .push(Router::with_path(r"avatars/{width|\d+}x{height|\d+}.{ext}").goal(fake_handler));
        let mut req = TestClient::get("http://local.host/avatars/321x641f.webp").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_none());

        let mut req = TestClient::get("http://local.host/avatars/320x640.webp").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
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
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
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
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
        let mut req =
            TestClient::get("http://local.host/user/726d694c-7af0-4bb0-9d22-706f7e386e").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_none());
    }

    #[tokio::test]
    async fn test_router_detect_method_mismatch_wildcard_sibling() {
        // Regression test for https://github.com/salvo-rs/salvo/issues/1612
        //
        // When a sibling route matches the path (capturing a wildcard param) but
        // fails a later filter such as the method filter, its captured params must
        // be rolled back. Otherwise the next sibling's wildcard insertion panics
        // with "only one wildcard param is allowed and it must be the last one".
        let router = Router::new()
            .push(Router::with_path("{foo|[a-f0-9]{4}}/{**subpath}").get(fake_handler))
            .push(Router::with_path("{**subpath}").get(fake_handler));

        // A HEAD request does not match the GET method filter of the first route.
        let mut req = TestClient::head("http://local.host/b33f/subpath.txt").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        // Must not panic, and should fall through with no matched goal.
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_none());

        // A GET request still matches the first route as before.
        let mut req = TestClient::get("http://local.host/b33f/subpath.txt").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
        assert_eq!(path_state.params["foo"], "b33f");
        assert_eq!(path_state.params["subpath"], "subpath.txt");
    }

    #[tokio::test]
    async fn test_router_detect_path_encoded() {
        let router = Router::new().path("api/{p}").get(fake_handler);
        let mut req = TestClient::get("http://127.0.0.1:6060/api/a%2fb%2fc").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
        assert_eq!(path_state.params["p"], "a/b/c");
    }

    #[tokio::test]
    async fn test_router_detect_sibling_param_rollback() {
        // A sibling captures a named param but then fails on a nested path, so its
        // capture must be rolled back before the next sibling is tried. Only the
        // params captured along the finally-matched branch may remain.
        let router = Router::new().push(
            Router::with_path("users").push(
                Router::new()
                    // Tried first: captures `id`, then fails because it requires a
                    // deeper `/profile` segment the request does not have.
                    .push(Router::with_path("{id}/profile").get(fake_handler))
                    // Matches: captures `name`.
                    .push(Router::with_path("{name}").get(fake_handler)),
            ),
        );

        let mut req = TestClient::get("http://local.host/users/alice").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
        // The winning branch's param is present...
        assert_eq!(path_state.params["name"], "alice");
        // ...and the failed sibling's `id` capture did not leak through.
        assert!(!path_state.params.contains_key("id"));
        assert_eq!(path_state.params.len(), 1);
    }

    #[tokio::test]
    async fn test_router_detect_overwritten_param_rollback() {
        // An ancestor captures `id`, then a failed descendant sibling reuses the same
        // name and overwrites it in place before failing. The ancestor's original value
        // must be restored so the sibling that finally matches sees the correct params.
        let router = Router::with_path("{id}")
            // Tried first: captures `id` again (overwriting the ancestor's value) then
            // fails because the request has no trailing `/profile`.
            .push(Router::with_path("{id}/profile").get(fake_handler))
            // Matches on the trailing literal segment, capturing no param of its own.
            .push(Router::with_path("edit").get(fake_handler));

        let mut req = TestClient::get("http://local.host/alice/edit").build();
        let mut path_state = PathState::from_owned_path(req.uri().path().to_owned());
        let matched = router.detect(&mut req, &mut path_state).await;
        assert!(matched.is_some());
        // Must be the ancestor's value, not the overwritten-then-rolled-back "edit".
        assert_eq!(path_state.params["id"], "alice");
    }
}
