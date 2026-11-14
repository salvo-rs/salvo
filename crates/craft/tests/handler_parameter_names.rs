//! Crafted wrappers do not reserve extractor parameter names.
use std::sync::Arc;

use salvo::oapi::extract::QueryParam;
use salvo::prelude::*;
use salvo::test::{ResponseExt, TestClient};
use salvo_craft::craft;

#[derive(Clone)]
struct Service;
#[craft]
impl Service {
    #[craft(handler)]
    fn reply(&self, handle: QueryParam<String>) -> String {
        handle.into_inner()
    }
    #[craft(endpoint)]
    fn documented(self: Arc<Self>, handle: QueryParam<String>) -> String {
        handle.into_inner()
    }
    #[craft(handler)]
    fn borrowed(self: &Arc<Self>, handle: QueryParam<String>) -> String {
        handle.into_inner()
    }
}

#[tokio::test]
async fn handle_is_an_ordinary_request_parameter_name() {
    let service = Arc::new(Service);
    let router = Router::new()
        .push(Router::with_path("reply").get(service.reply()))
        .push(Router::with_path("documented").get(service.documented()))
        .push(Router::with_path("borrowed").get(service.borrowed()));
    let document = salvo::oapi::OpenApi::new("parameter names", "1").merge_router(&router);
    assert!(document.paths.contains_key("/documented"));
    let server = salvo::Service::new(router);
    for path in ["reply", "documented", "borrowed"] {
        assert_eq!(
            TestClient::get(format!("http://localhost/{path}?handle=hello"))
                .send(&server)
                .await
                .take_string()
                .await
                .unwrap(),
            "hello"
        );
    }
}
