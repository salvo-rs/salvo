//! basic auth middleware
use std::time::Duration;

use salvo_core::http::{Request, Response, StatusError};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};

/// Timeout
pub struct Timeout {
    value: Duration,
}
impl Timeout {
    /// Create a new `Timeout`.
    #[inline]
    pub fn new(value: Duration) -> Self {
        Timeout { value }
    }
}
#[async_trait]
impl Handler for Timeout {
    #[inline]
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        tokio::select! {
            _ = ctrl.call_next(req, depot, res) => {},
            _ = tokio::time::sleep(self.value) => {
                res.render(StatusError::internal_server_error().brief("Server process the request timeout."));
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
