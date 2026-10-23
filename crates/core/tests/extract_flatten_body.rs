//! Flattened extractors must initialize the body required by their descendants.

#![cfg(feature = "test")]

use salvo_core::http::StatusCode;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Router, Service, Writer, handler};
use serde::Deserialize;

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "body")))]
struct BodyData {
    #[serde(default)]
    value: String,
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct Middle {
    #[salvo(extract(flatten))]
    body: BodyData,
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Outer {
    token: String,
    #[salvo(extract(flatten))]
    middle: Middle,
}

#[handler]
async fn nested(data: Outer) -> String {
    format!("{}:{}", data.token, data.middle.body.value)
}

#[tokio::test]
async fn nested_flatten_initializes_descendant_body_default_source() {
    let service = Service::new(Router::new().post(nested));
    for (content_type, body) in [
        ("application/json", r#"{"value":"accepted"}"#),
        ("application/x-www-form-urlencoded", "value=accepted"),
        (
            "multipart/form-data; boundary=test",
            "--test\r\nContent-Disposition: form-data; name=\"value\"\r\n\r\naccepted\r\n--test--\r\n",
        ),
    ] {
        let mut response = TestClient::post("http://localhost/?token=query")
            .add_header("content-type", content_type, true)
            .body(body)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "query:accepted");
    }
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct FieldBodyData {
    #[salvo(extract(source(from = "body")))]
    value: String,
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct FieldOuter {
    #[salvo(extract(flatten))]
    body: FieldBodyData,
}

#[handler]
async fn nested_field(data: FieldOuter) -> String {
    data.body.value
}

#[tokio::test]
async fn flatten_initializes_descendant_body_field_source() {
    let service = Service::new(Router::new().post(nested_field));
    for (content_type, body) in [
        ("application/json", r#"{"value":"accepted"}"#),
        ("application/x-www-form-urlencoded", "value=accepted"),
    ] {
        let mut response = TestClient::post("http://localhost/")
            .add_header("content-type", content_type, true)
            .body(body)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "accepted");
    }
}
