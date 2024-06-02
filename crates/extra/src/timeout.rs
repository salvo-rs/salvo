//! Middleware that provides support for timeout.
//!
//! Read more: <https://salvo.rs>
use std::time::Duration;

use salvo_core::http::{Request, Response, StatusError};
use salvo_core::http::headers::{HeaderMapExt, Connection};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};

/// Timeout
pub struct Timeout {
    value: Duration,
    error: Box<dyn Fn() -> StatusError + Send + Sync + 'static>,
}
impl Timeout {
    /// Create a new `Timeout`.
    #[inline]
    pub fn new(value: Duration) -> Self {
        // If a 408 error code is returned, the browser may resend the request multiple times. In most cases, this behavior is undesirable.
        // https://github.com/tower-rs/tower-http/issues/300
        Timeout { value, error:  Box::new(||StatusError::service_unavailable().brief("Server process the request timeout."))}
    }

    /// Custom error returned when timeout.
    pub fn error(mut self, error: impl Fn() -> StatusError + Send + Sync + 'static) -> Self {
        self.error = Box::new(error);
        self
    }
}
#[async_trait]
impl Handler for Timeout {
    #[inline]
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        tokio::select! {
            _ = ctrl.call_next(req, depot, res) => {},
            _ = tokio::time::sleep(self.value) => {
                res.headers_mut().typed_insert(Connection::close());
                res.render((self.error)());
                ctrl.skip_rest();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use super::*;

    #[tokio::test]
    async fn test_timeout_handler() {
        #[handler]
        async fn fast() -> &'static str {
            "hello"
        }
        #[handler]
        async fn slow() -> &'static str {
            tokio::time::sleep(Duration::from_secs(6)).await;
            "hello"
        }

        let router = Router::new()
            .hoop(Timeout::new(Duration::from_secs(5)))
            .push(Router::with_path("slow").get(slow))
            .push(Router::with_path("fast").get(fast));
        let service = Service::new(router);

        let content = TestClient::get("http://127.0.0.1:5801/slow")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("timeout"));

        let content = TestClient::get("http://127.0.0.1:5801/fast")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("hello"));
    }
}
