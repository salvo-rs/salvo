use std::sync::Arc;

use crate::http::{Request, Response};
use crate::{Depot, Handler};

/// Control the flow of execute handlers.
///
/// When a request is coming, [`Router`] will detect it and get the matched router.
/// And then salvo will collect all handlers (including added as middlewares) from the matched router tree.
/// All handlers in this list will executed one by one.
///
/// Each handler can use `FlowCtrl` to control execute flow, let the flow call next handler or skip all rest handlers.
///
/// **NOTE**: When `Response`'s status code is set, and the status code [`Response::is_stamped()`] is returns false,
/// all remaining handlers will be skipped.
///
/// [`Router`]: crate::routing::Router
#[derive(Default)]
pub struct FlowCtrl {
    catching: Option<bool>,
    is_ceased: bool,
    pub(crate) cursor: usize,
    pub(crate) handlers: Vec<Arc<dyn Handler>>,
}

impl FlowCtrl {
    /// Create new `FlowCtrl`.
    #[inline]
    pub fn new(handlers: Vec<Arc<dyn Handler>>) -> Self {
        FlowCtrl {
            catching: None,
            is_ceased: false,
            cursor: 0,
            handlers,
        }
    }
    /// Has next handler.
    #[inline]
    pub fn has_next(&self) -> bool {
        self.cursor < self.handlers.len() // && !self.handlers.is_empty()
    }

    /// Call next handler. If get next handler and executed, returns `true``, otherwise returns `false`.
    ///
    /// **NOTE**: If response status code is error or is redirection, all reset handlers will be skipped.
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
        let mut handler = self.handlers.get(self.cursor).cloned();
        if handler.is_none() {
            false
        } else {
            while let Some(h) = handler.take() {
                self.cursor += 1;
                h.handle(req, depot, res, self).await;
                if !self.catching.unwrap_or_default() && res.is_stamped() {
                    self.skip_rest();
                    return true;
                } else if self.has_next() {
                    handler = self.handlers.get(self.cursor).cloned();
                }
            }
            true
        }
    }

    /// Skip all reset handlers.
    #[inline]
    pub fn skip_rest(&mut self) {
        self.cursor = self.handlers.len()
    }

    /// Check is `FlowCtrl` ceased.
    ///
    /// **NOTE**: If handler is used as middleware, it should use `is_ceased` to check is flow ceased.
    /// If `is_ceased` returns `true`, the handler should skip the following logic.
    #[inline]
    pub fn is_ceased(&self) -> bool {
        self.is_ceased
    }
    /// Cease all following logic.
    ///
    /// **NOTE**: This function will mark is_ceased as `true`, but whether the subsequent logic can be skipped
    /// depends on whether the middleware correctly checks is_ceased and skips the subsequent logic.
    #[inline]
    pub fn cease(&mut self) {
        self.skip_rest();
        self.is_ceased = true;
    }
}
