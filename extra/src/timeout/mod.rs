//! basic auth middleware
use std::time::Duration;

use salvo_core::async_trait;
use salvo_core::http::{Request, Response, StatusError};
use salvo_core::routing::FlowCtrl;
use salvo_core::{Depot, Handler};

/// TimeoutHandler
pub struct TimeoutHandler {
    timeout: Duration,
}
impl TimeoutHandler {
    /// Create a new `TimeoutHandler`.
    #[inline]
    pub fn new(timeout: Duration) -> Self {
        TimeoutHandler { timeout }
    }
}
#[async_trait]
impl Handler for TimeoutHandler {
    #[inline]
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        tokio::select! {
            _ = ctrl.call_next(req, depot, res) => {},
            _ = tokio::time::sleep(self.timeout) => {
                res.set_status_error(StatusError::internal_server_error().with_detail("Server process the request timeout."))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::hyper;
    use salvo_core::prelude::*;

    use super::*;

    #[tokio::test]
    async fn test_timeout_handler() {
        #[fn_handler]
        async fn fast() -> &'static str {
            "hello"
        }
        #[fn_handler]
        async fn slow() -> &'static str {
            tokio::time::sleep(Duration::from_secs(6)).await;
            "hello"
        }

        let router = Router::new()
            .hoop(TimeoutHandler::new(Duration::from_secs(5)))
            .push(Router::with_path("slow").get(slow))
            .push(Router::with_path("fast").get(fast));
        let service = Service::new(router);

        let req = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/slow").body(hyper::Body::empty()).unwrap();
        let content = service.handle(req).await.take_text().await.unwrap();
        assert!(content.contains("timeout"));

        let req = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/fast").body(hyper::Body::empty()).unwrap();
        let content = service.handle(req).await.take_text().await.unwrap();
        assert!(content.contains("hello"));
    }
}
