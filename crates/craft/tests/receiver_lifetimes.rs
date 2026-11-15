//! Explicit immutable receiver lifetimes keep their ordinary Rust meaning.
use std::sync::Arc;

use salvo::prelude::*;
use salvo_craft::craft;

#[derive(Clone)]
struct Service;
#[craft]
impl Service {
    #[craft(handler)]
    fn typed<'a>(self: &'a Self) -> &'static str {
        "typed"
    }
    #[craft(handler)]
    fn shorthand<'a>(&'a self) -> &'static str {
        "shorthand"
    }
    #[craft(handler)]
    fn arc<'a>(self: &'a Arc<Self>) -> &'static str {
        "arc"
    }
    #[craft(endpoint)]
    fn qualified_arc<'a>(self: &'a std::sync::Arc<Self>) -> &'static str {
        "qualified"
    }
}

#[tokio::test]
async fn explicit_self_and_arc_references_make_usable_handlers() {
    use salvo::test::{ResponseExt, TestClient};
    let service = Arc::new(Service);
    let router = Router::new()
        .push(Router::with_path("typed").get(service.typed()))
        .push(Router::with_path("shorthand").get(service.shorthand()))
        .push(Router::with_path("arc").get(service.arc()))
        .push(Router::with_path("qualified").get(service.qualified_arc()));
    let doc = salvo::oapi::OpenApi::new("receiver", "1").merge_router(&router);
    assert!(doc.paths.contains_key("/qualified"));
    let server = salvo::Service::new(router);
    for path in ["typed", "shorthand", "arc", "qualified"] {
        assert_eq!(
            TestClient::get(format!("http://localhost/{path}"))
                .send(&server)
                .await
                .take_string()
                .await
                .unwrap(),
            path
        );
    }
}
