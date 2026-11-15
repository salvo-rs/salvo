//! Unit and manually written endpoints retain valid, honest response metadata.
#![allow(clippy::unused_unit, unused_parens)]
use salvo_core::http::{StatusCode, StatusError};
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Response, Router};
use salvo_oapi::{OpenApi, endpoint};
use serde_json::json;

#[endpoint]
fn explicit() -> () {}
#[endpoint]
fn implicit() {}
#[endpoint]
fn parenthesized() -> (()) {}
macro_rules! unit_endpoint {
    ($output:ty) => {
        #[endpoint]
        fn grouped() -> $output {}
    };
}
unit_endpoint!(());
#[endpoint]
fn created(res: &mut Response) {
    res.status_code(StatusCode::CREATED);
    res.render("created");
}
#[endpoint]
fn missing(res: &mut Response) {
    res.status_code(StatusCode::NOT_FOUND);
    res.render("missing");
}
#[endpoint(responses((status_code = 201, description = "Created")), status_codes(StatusCode::CREATED))]
fn documented(res: &mut Response) {
    res.status_code(StatusCode::CREATED);
}
#[endpoint(status_codes(StatusCode::NO_CONTENT))]
fn filtered() -> &'static str {
    "filtered"
}
#[endpoint]
fn result_unit() -> Result<(), StatusError> {
    Ok(())
}

#[tokio::test]
async fn untyped_output_uses_fallback_only_when_no_responses_remain() {
    let router = Router::new()
        .push(Router::with_path("explicit").get(explicit))
        .push(Router::with_path("implicit").get(implicit))
        .push(Router::with_path("parenthesized").get(parenthesized))
        .push(Router::with_path("grouped").get(grouped))
        .push(Router::with_path("created").get(created))
        .push(Router::with_path("missing").get(missing))
        .push(Router::with_path("documented").get(documented))
        .push(Router::with_path("filtered").get(filtered))
        .push(Router::with_path("result").get(result_unit));
    let doc = json!(OpenApi::new("unit", "1").merge_router(&router));
    for path in [
        "explicit",
        "implicit",
        "parenthesized",
        "grouped",
        "created",
        "missing",
        "filtered",
    ] {
        let responses = &doc["paths"][format!("/{path}")]["get"]["responses"];
        assert_eq!(responses.as_object().unwrap().len(), 1);
        assert!(responses["default"]["description"].as_str().is_some());
        assert!(responses["default"].get("content").is_none());
    }
    assert_eq!(
        doc["paths"]["/explicit"]["get"]["responses"],
        doc["paths"]["/implicit"]["get"]["responses"]
    );
    let described = &doc["paths"]["/documented"]["get"]["responses"];
    assert_eq!(described.as_object().unwrap().len(), 1);
    assert_eq!(described["201"]["description"], "Created");
    let result = &doc["paths"]["/result"]["get"]["responses"];
    assert!(result.get("200").is_some());
    assert!(result.get("400").is_some());
    assert!(result.get("default").is_none());
    let service = salvo_core::Service::new(router);
    for (path, status) in [
        ("created", StatusCode::CREATED),
        ("missing", StatusCode::NOT_FOUND),
    ] {
        let mut response = TestClient::get(format!("http://localhost/{path}"))
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(status));
        assert_eq!(response.take_string().await.unwrap(), path);
    }
    let explicit_response = TestClient::get("http://localhost/explicit")
        .send(&service)
        .await;
    let implicit_response = TestClient::get("http://localhost/implicit")
        .send(&service)
        .await;
    assert_eq!(explicit_response.status_code, implicit_response.status_code);
}
