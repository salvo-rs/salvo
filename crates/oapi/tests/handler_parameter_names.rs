//! Handler value names must not turn argument bindings into unit-struct patterns.
#![deny(non_local_definitions)]
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Response, Router, handler};
use salvo_oapi::extract::QueryParam;
use salvo_oapi::{OpenApi, endpoint};

#[handler]
fn lookup(lookup: QueryParam<String>) -> String {
    let _handler = Self;
    format!("{lookup}")
}
#[endpoint]
async fn search(search: QueryParam<String>) -> String {
    let _handler = Self;
    format!("{search}")
}
#[handler]
fn native(native: &mut Response) {
    native.render("native");
}
#[endpoint]
fn r#type(r#type: QueryParam<String>) -> String {
    format!("{}", r#type)
}

#[tokio::test]
async fn equal_names_keep_extraction_metadata_and_public_handler_values() {
    let router = Router::new()
        .push(Router::with_path("lookup").get(lookup))
        .push(Router::with_path("search").get(search))
        .push(Router::with_path("native").get(native))
        .push(Router::with_path("raw").get(r#type));
    let document = serde_json::to_value(OpenApi::new("names", "1").merge_router(&router)).unwrap();
    assert_eq!(
        document["paths"]["/search"]["get"]["parameters"][0]["name"],
        "search"
    );
    assert_eq!(
        document["paths"]["/raw"]["get"]["parameters"][0]["name"],
        "type"
    );
    let service = salvo_core::Service::new(router);
    for (path, expected) in [
        ("lookup?lookup=found", "found"),
        ("search?search=hello", "hello"),
        ("native", "native"),
        ("raw?type=raw", "raw"),
    ] {
        let body = TestClient::get(format!("http://localhost/{path}"))
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert_eq!(body, expected);
    }
}
