//! size limiter middleware

use salvo_core::async_trait;
use salvo_core::http::errors::StatusError;
use salvo_core::http::HttpBody;
use salvo_core::prelude::*;

/// MaxSizeHandler
pub struct MaxSizeHandler(u64);
#[async_trait]
impl Handler for MaxSizeHandler {
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
pub fn max_size(size: u64) -> MaxSizeHandler {
    MaxSizeHandler(size)
}

#[cfg(test)]
mod tests {
    use salvo_core::hyper;
    use salvo_core::prelude::*;

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

        let req: Request = hyper::Request::builder()
            .method("POST")
            .uri("http://127.0.0.1:7979/hello")
            .body("abc".into())
            .unwrap()
            .into();
        let content = service.handle(req).await.take_text().await.unwrap();
        assert_eq!(content, "hello");

        let req: Request = hyper::Request::builder()
            .method("POST")
            .uri("http://127.0.0.1:7979/hello")
            .body("abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz".into())
            .unwrap()
            .into();
        let res = service.handle(req).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
