//! Endpoint metadata follows the same cfg conditions as its handle method.
use salvo_core::Router;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_oapi::{OpenApi, endpoint};

struct Conditional;
#[endpoint]
impl Conditional {
    #[cfg(all())]
    fn handle(&self) -> &'static str {
        "direct"
    }
    #[cfg(any())]
    fn handle(&self, _invalid: MissingExtractor) -> MissingResponse {
        unreachable!()
    }
}

struct Nested;
#[endpoint]
impl Nested {
    #[cfg_attr(all(), inline, cfg(all()))]
    fn handle(&self) -> &'static str {
        "nested"
    }
    #[cfg_attr(all(), cfg_attr(all(), cfg(any())), inline)]
    fn handle(&self, _invalid: &[u32]) -> &'static str {
        "disabled"
    }
}

struct MetadataConditional;
#[endpoint]
impl MetadataConditional {
    #[cfg(all())]
    fn handle(&self) -> &'static str {
        "metadata"
    }
    #[cfg(any())]
    fn handle(&self) -> u32 {
        7
    }
}

struct Disabled;
#[endpoint]
impl Disabled {
    #[cfg(any())]
    fn handle(&self, _invalid: MissingExtractor) -> MissingResponse {
        unreachable!()
    }
}

#[tokio::test]
async fn configured_endpoint_handlers_and_metadata_agree() {
    assert!(salvo_oapi::EndpointRegistry::find(&std::any::TypeId::of::<Disabled>()).is_none());
    assert!(salvo_oapi::EndpointRegistry::find(&std::any::TypeId::of::<Conditional>()).is_some());
    let router = Router::new()
        .push(Router::with_path("direct").get(Conditional))
        .push(Router::with_path("nested").get(Nested))
        .push(Router::with_path("metadata").get(MetadataConditional));
    let document = serde_json::json!(OpenApi::new("conditional", "1").merge_router(&router));
    let service = salvo_core::Service::new(router);
    for (path, expected) in [
        ("direct", "direct"),
        ("nested", "nested"),
        ("metadata", "metadata"),
    ] {
        assert_eq!(
            document["paths"][format!("/{path}")]["get"]["responses"]["200"]["content"]["text/plain"]
                ["schema"]["type"],
            "string"
        );
        let response = TestClient::get(format!("http://localhost/{path}"))
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(response, expected);
    }
}
