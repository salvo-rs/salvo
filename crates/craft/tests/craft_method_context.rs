//! Crafted method bodies retain the service's lexical and receiver context.
use std::sync::Arc;

use salvo::prelude::*;
use salvo::test::{ResponseExt, TestClient};
use salvo_craft::craft;

#[derive(Clone, Debug)]
struct Service;

#[craft]
impl Service {
    const LABEL: &'static str = "service";

    #[craft(handler)]
    fn label(&self) -> &'static str {
        Self::LABEL
    }

    #[craft(endpoint)]
    fn plain() -> &'static str {
        Self::LABEL
    }

    #[craft(handler)]
    fn owned(self: Arc<Self>) -> String {
        Arc::strong_count(&self).to_string()
    }

    #[craft(endpoint)]
    fn borrowed(self: &Arc<Self>) -> String {
        Arc::strong_count(self).to_string()
    }
}

#[tokio::test]
async fn self_resolves_to_the_service_in_both_method_kinds() {
    for handler in [
        Router::new().get(Service.label()),
        Router::new().get(Service::plain()),
    ] {
        let body = TestClient::get("http://localhost/")
            .send(handler)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(body, "service");
    }
}

#[tokio::test]
async fn arc_receivers_retain_their_declared_types() {
    let service = Arc::new(Service);
    let router = Router::new().get(service.owned());
    let body = TestClient::get("http://localhost/")
        .send(router)
        .await
        .take_string()
        .await
        .unwrap();
    assert_eq!(body, "3");
    let router = Router::new().get(service.borrowed());
    let body = TestClient::get("http://localhost/")
        .send(router)
        .await
        .take_string()
        .await
        .unwrap();
    assert_eq!(body, "2");
}
