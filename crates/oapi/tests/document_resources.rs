//! Frozen documentation responses share their bytes; metadata stays on routers.
use salvo_core::http::ResBody;
use salvo_core::test::TestClient;
use salvo_core::{Depot, FlowCtrl, Handler, Response, Router};
use salvo_oapi::{OpenApi, RouterExt};

async fn body(handler: &impl Handler, url: &str) -> salvo_core::hyper::body::Bytes {
    let mut req = TestClient::get(url).build();
    let mut response = Response::new();
    handler
        .handle(
            &mut req,
            &mut Depot::new(),
            &mut response,
            &mut FlowCtrl::default(),
        )
        .await;
    assert_eq!(
        response.headers()["content-type"],
        "application/json; charset=utf-8"
    );
    let ResBody::Once(bytes) = response.take_body() else {
        panic!("expected shared bytes")
    };
    bytes
}

#[tokio::test]
async fn frozen_handler_shares_each_serialized_representation() {
    let doc = OpenApi::new("test", "1");
    let compact = doc.to_json().unwrap();
    let pretty = doc.to_pretty_json().unwrap();
    let handler = doc.into_handler();
    let a = body(&handler, "http://localhost/").await;
    let b = body(&handler, "http://localhost/?pretty=false").await;
    assert_eq!(a, compact);
    assert_eq!(a.as_ptr(), b.as_ptr());
    let a = body(&handler, "http://localhost/?pretty=true").await;
    let b = body(&handler, "http://localhost/?pretty").await;
    assert_eq!(a, pretty);
    assert_eq!(a.as_ptr(), b.as_ptr());
}

#[salvo_oapi::endpoint]
async fn endpoint() -> &'static str {
    "ok"
}

#[test]
fn router_metadata_is_owned_without_runtime_hoops_and_keeps_inheritance() {
    let child = Router::with_path("items")
        .oapi_tag("child")
        .oapi_tag("child")
        .get(endpoint);
    let router = Router::new()
        .oapi_tag("root")
        .oapi_tags(["shared"])
        .oapi_security(salvo_oapi::SecurityRequirement::new("token", ["read"]))
        .push(child);
    assert!(router.hoops.is_empty());
    assert!(router.routers[0].hoops.is_empty());
    let a = OpenApi::new("test", "1").merge_router(&router);
    let b = OpenApi::new("test", "1").merge_router(&router);
    let a = serde_json::to_value(a).unwrap();
    assert_eq!(a, serde_json::to_value(b).unwrap());
    assert_eq!(
        a["paths"]["/items"]["get"]["tags"],
        serde_json::json!(["child", "root", "shared"])
    );
    assert_eq!(
        a["paths"]["/items"]["get"]["security"],
        serde_json::json!([{"token":["read"]}])
    );
}

#[tokio::test]
#[ignore = "manual serialization cost comparison"]
async fn frozen_document_serialization_cost() {
    let mut doc = OpenApi::new("measure", "1");
    for index in 0..1000 {
        doc = doc.add_schema(
            format!("Item{index}"),
            salvo_oapi::Object::new().description("benchmark schema"),
        );
    }
    let handler = doc.clone().into_handler();
    let _ = body(&handler, "http://localhost/").await;
    let start = std::time::Instant::now();
    for _ in 0..100 {
        std::hint::black_box(doc.to_json().unwrap());
    }
    let repeated = start.elapsed();
    let start = std::time::Instant::now();
    for _ in 0..100 {
        std::hint::black_box(body(&handler, "http://localhost/").await);
    }
    eprintln!(
        "100 document serializations={repeated:?}; 100 cached response constructions={:?}",
        start.elapsed()
    );
}
