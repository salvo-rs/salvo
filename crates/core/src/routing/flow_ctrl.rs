use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;

use crate::http::{Request, Response};
use crate::{Depot, Handler};

/// Controls execution of a matched handler chain.
///
/// When a request arrives, [`Router`] matches it against the routing tree. Salvo
/// then collects the matched goal handler and all middleware handlers from the
/// matched route branch. Handlers in that list are executed in order.
///
/// Most middleware only needs to read or write request state and then return.
/// Use [`FlowCtrl::call_next`] for around-style middleware that needs to run
/// logic after later handlers have finished, or [`FlowCtrl::skip_rest`] to stop
/// the remaining handlers.
///
/// **Note:** when the response becomes stamped, remaining handlers are skipped.
/// See [`Response::is_stamped`] for the exact status-code behavior.
///
/// # Example
///
/// ```
/// use salvo_core::http::header::{HeaderValue, SERVER};
/// use salvo_core::prelude::*;
///
/// #[handler]
/// async fn add_server_header(
///     req: &mut Request,
///     depot: &mut Depot,
///     res: &mut Response,
///     ctrl: &mut FlowCtrl,
/// ) {
///     ctrl.call_next(req, depot, res).await;
///     if !ctrl.is_ceased() {
///         res.headers_mut()
///             .insert(SERVER, HeaderValue::from_static("salvo"));
///     }
/// }
/// ```
///
/// [`Router`]: crate::routing::Router
#[derive(Default)]
pub struct FlowCtrl {
    catching: Option<bool>,
    is_ceased: bool,
    pub(crate) cursor: usize,
    pub(crate) handlers: Vec<Option<Arc<dyn Handler>>>,
}

impl Debug for FlowCtrl {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("FlowCtrl")
            .field("catching", &self.catching)
            .field("is_ceased", &self.is_ceased)
            .field("cursor", &self.cursor)
            .finish()
    }
}

impl FlowCtrl {
    /// Creates a new `FlowCtrl`.
    #[inline]
    #[must_use]
    pub fn new(handlers: Vec<Arc<dyn Handler>>) -> Self {
        Self {
            catching: None,
            is_ceased: false,
            cursor: 0,
            handlers: handlers.into_iter().map(Some).collect(),
        }
    }
    /// Returns whether there is another handler in the chain.
    #[inline]
    #[must_use]
    pub fn has_next(&self) -> bool {
        self.cursor < self.handlers.len()
    }

    /// Runs the next handler in the chain.
    ///
    /// Returns `true` if at least one handler ran, and `false` if there was no
    /// remaining handler to dispatch to.
    ///
    /// **NOTE**: If the response is already in a terminal state (an error or
    /// redirection status code, as reported by [`Response::is_stamped`]) when this
    /// method is called — or becomes terminal after a handler runs — the remaining
    /// handlers are skipped. The first call to `call_next` latches whether catcher
    /// mode is active, so subsequent calls behave consistently within the same
    /// request.
    ///
    /// [`Response::is_stamped`]: crate::http::Response::is_stamped
    #[inline]
    pub async fn call_next(
        &mut self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
    ) -> bool {
        if self.catching.is_none() {
            self.catching = Some(res.is_stamped());
        }
        if !self.catching.unwrap_or_default() && res.is_stamped() {
            self.skip_rest();
            return false;
        }
        let start = self.cursor;
        while self.cursor < self.handlers.len() {
            let handler = self.handlers[self.cursor].take();
            self.cursor += 1;
            if let Some(handler) = handler {
                handler.handle(req, depot, res, self).await;
                if !self.catching.unwrap_or_default() && res.is_stamped() {
                    self.skip_rest();
                    return true;
                }
            }
        }
        self.cursor > start
    }

    /// Skip all remaining handlers.
    #[inline]
    pub fn skip_rest(&mut self) {
        self.cursor = self.handlers.len()
    }

    /// Checks whether the handler chain has been ceased.
    ///
    /// **Note:** around-style middleware should check this after
    /// [`FlowCtrl::call_next`] and skip post-processing when it returns `true`.
    #[inline]
    #[must_use]
    pub fn is_ceased(&self) -> bool {
        self.is_ceased
    }
    /// Ceases the remaining handler chain.
    ///
    /// This marks the flow as ceased and skips all remaining handlers. Middleware
    /// that has already called [`FlowCtrl::call_next`] should still check
    /// [`FlowCtrl::is_ceased`] before running any post-processing.
    #[inline]
    pub fn cease(&mut self) {
        self.skip_rest();
        self.is_ceased = true;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::prelude::*;
    use crate::routing::FlowCtrl;
    use crate::{Depot, Handler, Request, Response};

    #[tokio::test]
    async fn test_reentrant_call_next() {
        #[handler]
        async fn around_a(
            req: &mut Request,
            depot: &mut Depot,
            res: &mut Response,
            ctrl: &mut FlowCtrl,
        ) {
            depot.get_typed_mut::<Vec<&str>>().unwrap().push("enter_a");
            assert!(ctrl.call_next(req, depot, res).await);
            depot.get_typed_mut::<Vec<&str>>().unwrap().push("exit_a");
        }

        #[handler]
        async fn around_b(
            req: &mut Request,
            depot: &mut Depot,
            res: &mut Response,
            ctrl: &mut FlowCtrl,
        ) {
            depot.get_typed_mut::<Vec<&str>>().unwrap().push("enter_b");
            assert!(ctrl.call_next(req, depot, res).await);
            depot.get_typed_mut::<Vec<&str>>().unwrap().push("exit_b");
        }

        #[handler]
        async fn goal(res: &mut Response, depot: &mut Depot) {
            depot.get_typed_mut::<Vec<&str>>().unwrap().push("goal");
            res.status_code(StatusCode::OK);
        }

        let order: Vec<&str> = Vec::new();
        let handlers: Vec<Arc<dyn Handler>> =
            vec![Arc::new(around_a), Arc::new(around_b), Arc::new(goal)];

        let mut req = Request::new();
        let mut depot = Depot::new();
        depot.insert_typed(order);
        let mut res = Response::new();
        let mut ctrl = FlowCtrl::new(handlers);
        let ran = ctrl.call_next(&mut req, &mut depot, &mut res).await;

        assert!(ran);
        assert_eq!(res.status_code, Some(StatusCode::OK));
        assert!(!ctrl.has_next());
        assert_eq!(
            depot.get_typed::<Vec<&str>>().unwrap(),
            &["enter_a", "enter_b", "goal", "exit_b", "exit_a"]
        );
    }

    #[tokio::test]
    async fn test_empty_handler_chain() {
        let handlers: Vec<Arc<dyn Handler>> = Vec::new();
        let mut req = Request::new();
        let mut depot = Depot::new();
        let mut res = Response::new();
        let mut ctrl = FlowCtrl::new(handlers);
        assert!(!ctrl.call_next(&mut req, &mut depot, &mut res).await);
        assert!(!ctrl.has_next());
    }
}
