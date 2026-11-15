//! Redundant and macro_rules type groups preserve extractor/context semantics.
#![allow(unused_parens)]
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Request, Router};
use salvo_oapi::extract::QueryParam;
use salvo_oapi::{OpenApi, endpoint};
use serde_json::json;

macro_rules! handlers {
    ($extractor:ty, $request:ty) => {
        #[salvo_core::handler]
        fn grouped(value: $extractor, req: $request) -> String {
            assert_eq!(req.uri().path(), "/grouped");
            value.into_inner()
        }
        #[endpoint]
        fn documented(value: $extractor, _req: $request) -> String {
            value.into_inner()
        }
    };
}
handlers!(QueryParam<String>, &mut Request);

#[salvo_core::handler]
fn parenthesized(value: ((QueryParam<String>)), _req: (&mut (Request))) -> String {
    value.into_inner()
}
#[endpoint]
fn documented_parentheses(value: ((QueryParam<String>)), _req: (&mut (Request))) -> String {
    value.into_inner()
}

#[tokio::test]
async fn grouped_types_extract_and_register_normally() {
    let router = Router::new()
        .push(Router::with_path("grouped").get(grouped))
        .push(Router::with_path("documented").get(documented))
        .push(Router::with_path("parenthesized").get(parenthesized))
        .push(Router::with_path("documented_parentheses").get(documented_parentheses));
    let doc = json!(OpenApi::new("grouped", "1").merge_router(&router));
    assert_eq!(
        doc["paths"]["/documented"]["get"]["parameters"][0]["name"],
        "value"
    );
    assert_eq!(
        doc["paths"]["/documented_parentheses"]["get"]["parameters"][0]["name"],
        "value"
    );
    let service = salvo_core::Service::new(router);
    for path in [
        "grouped",
        "documented",
        "parenthesized",
        "documented_parentheses",
    ] {
        assert_eq!(
            TestClient::get(format!("http://localhost/{path}?value=hello"))
                .send(&service)
                .await
                .take_string()
                .await
                .unwrap(),
            "hello"
        );
    }
}
