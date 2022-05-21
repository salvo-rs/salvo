//! Routing and filters
//! Router can route http requests to different handlers.

pub mod filter;
mod router;
pub use filter::*;
pub use router::{DetectMatched, Router};

use std::collections::HashMap;
use std::sync::Arc;

use crate::http::{Request, Response};
use crate::{Depot, Handler};

#[doc(hidden)]
pub type PathParams = HashMap<String, String>;
#[doc(hidden)]
#[derive(Debug, Eq, PartialEq)]
pub struct PathState {
    pub(crate) url_path: String,
    pub(crate) cursor: usize,
    pub(crate) params: PathParams,
}
impl PathState {
    /// Create new `PathState`.
    #[inline]
    pub fn new(url_path: &str) -> Self {
        let url_path = url_path.trim_start_matches('/').trim_end_matches('/');
        PathState {
            url_path: decode_url_path_safely(url_path),
            cursor: 0,
            params: PathParams::new(),
        }
    }
    #[inline]
    pub(crate) fn ended(&self) -> bool {
        self.cursor >= self.url_path.len()
    }
}

#[inline]
fn decode_url_path_safely(path: &str) -> String {
    percent_encoding::percent_decode_str(path)
        .decode_utf8_lossy()
        .to_string()
}

/// `FlowCtrl` is used to control the flow of execute handlers.
///
/// When a request is comming, [`Router`] will detect it and get the matched one.
/// And then salvo will collect all handlers (including added as middlewares) in a list.
/// All handlers in this list will executed one by one. Each handler can use `FlowCtrl` to control this
/// flow, let the flow call next handler or skip all rest handlers.
///
/// **NOTE**: When `Response`'s status code is set, and it's `is_success()` is returns false, all rest handlers
/// will skipped.
///
/// [`Router`]: crate::routing::Router
pub struct FlowCtrl {
    is_ceased: bool,
    cursor: usize,
    pub(crate) handlers: Vec<Arc<dyn Handler>>,
}

impl FlowCtrl {
    /// Create new `FlowCtrl`.
    #[inline]
    pub fn new(handlers: Vec<Arc<dyn Handler>>) -> Self {
        FlowCtrl {
            is_ceased: false,
            cursor: 0,
            handlers,
        }
    }
    /// Has next handler.
    #[inline]
    pub fn has_next(&self) -> bool {
        self.cursor < self.handlers.len() && !self.handlers.is_empty()
    }

    /// Call next handler. If get next handler and executed, returns true, otherwise returns false.
    ///
    /// If resposne's statuse code is error or is redirection, all reset handlers will skipped.
    #[inline]
    pub async fn call_next(&mut self, req: &mut Request, depot: &mut Depot, res: &mut Response) -> bool {
        if let Some(code) = res.status_code() {
            if code.is_client_error() || code.is_server_error() || code.is_redirection() {
                self.skip_rest();
                return false;
            }
        }
        if let Some(handler) = self.handlers.get(self.cursor) {
            self.cursor += 1;
            handler.clone().handle(req, depot, res, self).await;
            true
        } else {
            false
        }
    }
    /// Skip all reset handlers.
    #[inline]
    pub fn skip_rest(&mut self) {
        self.cursor = self.handlers.len()
    }

    /// Check is `FlowCtrl` ceased.
    #[inline]
    pub fn is_ceased(&self) -> bool {
        self.is_ceased
    }
    /// Cease all following logic.
    ///
    /// If handler is used as middleware, it should use `is_ceased` to check is flow is ceased.
    /// if `is_ceased` returns true, the handler should skip the following logic.
    #[inline]
    pub fn cease(&mut self) {
        self.skip_rest();
        self.is_ceased = true;
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use crate::test::{ResponseExt, TestClient};
    
    #[tokio::test]
    async fn test_custom_filter() {
        #[fn_handler(internal)]
        async fn hello_world() -> &'static str {
            "Hello World"
        }

        let router = Router::new()
            .filter_fn(|req, _| {
                let host = req.uri().host().unwrap_or_default();
                host == "localhost"
            })
            .get(hello_world);
        let service = Service::new(router);

        async fn access(service: &Service, host: &str) -> String {
            TestClient::get(format!("http://{}/", host))
                .send(service)
                .await
                .take_string()
                .await
                .unwrap()
        }

        assert!(access(&service, "127.0.0.1").await.contains("404: Not Found"));
        assert_eq!(access(&service, "localhost").await, "Hello World");
    }
}
