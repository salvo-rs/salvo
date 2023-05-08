//! size limiter middleware

use salvo_core::http::StatusError;
use salvo_core::http::{Body, Request, Response};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};

/// MaxSize
pub struct MaxSize(pub u64);
#[async_trait]
impl Handler for MaxSize {
    #[inline]
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        let size_hint = req.body().size_hint().upper();
        if let Some(upper) = size_hint {
            if upper > self.0 {
                res.render(StatusError::payload_too_large());
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        } else {
            res.render(StatusError::bad_request().detail("body size is unknown"));
            ctrl.skip_rest();
        }
    }
}
/// Create a new `MaxSize`.
#[inline]
pub fn max_size(size: u64) -> MaxSize {
    MaxSize(size)
}

#[cfg(test)]
mod tests {
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use super::*;

    #[handler]
    async fn hello() -> &'static str {
        "hello"
    }

    #[tokio::test]
    async fn test_size_limiter() {
        let limit_handler = MaxSize(32);
        let router = Router::new()
            .hoop(limit_handler)
            .push(Router::with_path("hello").post(hello));
        let service = Service::new(router);

        let content = TestClient::post("http://127.0.0.1:5801/hello")
            .text("abc")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(content, "hello");

        let res = TestClient::post("http://127.0.0.1:5801/hello")
            .text("abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
