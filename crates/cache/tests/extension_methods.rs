//! Only GET responses are cached by default, including for extension methods.
#![allow(missing_docs, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use salvo_cache::{Cache, MokaStore, RequestIssuer};
use salvo_core::http::Method;
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};

#[derive(Debug)]
struct Page(Arc<AtomicUsize>);
#[async_trait]
impl Handler for Page {
    async fn handle(&self, req: &mut Request, _: &mut Depot, res: &mut Response, _: &mut FlowCtrl) {
        res.render(format!(
            "{}:{}",
            req.method(),
            self.0.fetch_add(1, Ordering::SeqCst)
        ));
    }
}

async fn send(service: &Service, method: &str) -> String {
    let mut request = TestClient::get("http://localhost/").build();
    *request.method_mut() = Method::from_bytes(method.as_bytes()).unwrap();
    service.handle(request).await.take_string().await.unwrap()
}

#[tokio::test]
async fn extension_method_responses_are_not_stored_by_default() {
    for issuer in [RequestIssuer::new(), RequestIssuer::new().use_method(false)] {
        let service = Service::new(
            Router::new()
                .hoop(Cache::new(MokaStore::new(10), issuer))
                .goal(Page(Arc::new(AtomicUsize::new(0)))),
        );
        assert_eq!(send(&service, "FOO").await, "FOO:0");
        assert_eq!(send(&service, "FOO").await, "FOO:1");
        assert_eq!(send(&service, "GET").await, "GET:2");
        assert_eq!(send(&service, "GET").await, "GET:2");
        assert_eq!(send(&service, "FOO").await, "FOO:3");
    }
}
