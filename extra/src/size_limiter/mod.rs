//! size limiter middleware

use hyper::body::HttpBody;
use salvo_core::async_trait;
use salvo_core::http::StatusError;
use salvo_core::prelude::*;

/// MaxSizeHandler
pub struct MaxSizeHandler(u64);
#[async_trait]
impl Handler for MaxSizeHandler {
    #[inline]
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if let Some(upper) = req.body().and_then(|body| body.size_hint().upper()) {
            if upper > self.0 {
                res.set_status_error(StatusError::payload_too_large());
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        }
    }
}
/// Create a new ```MaxSizeHandler```.
#[inline]
pub fn max_size(size: u64) -> MaxSizeHandler {
    MaxSizeHandler(size)
}

#[cfg(test)]
mod tests {
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use super::*;

    #[fn_handler]
    async fn hello() -> &'static str {
        "hello"
    }

    #[tokio::test]
    async fn test_size_limiter() {
        let limit_handler = MaxSizeHandler(32);
        let router = Router::new()
            .hoop(limit_handler)
            .push(Router::with_path("hello").post(hello));
        let service = Service::new(router);

        let content = TestClient::post("http://127.0.0.1:7979/hello")
            .text("abc")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(content, "hello");

        let res = TestClient::post("http://127.0.0.1:7979/hello")
            .text("abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz")
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
