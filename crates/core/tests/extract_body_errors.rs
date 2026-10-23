//! Derived extractors must preserve request body read and parsing failures.

#![cfg(feature = "test")]

use salvo_core::http::StatusCode;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Request, Router, Service, Writer, handler};
use serde::Deserialize;

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "body")))]
struct BodyData {
    #[serde(default)]
    value: String,
}

#[handler]
async fn body_data(data: BodyData) -> String {
    format!("executed:{}", data.value)
}

#[handler]
async fn limit(req: &mut Request) {
    req.set_secure_max_size(8);
}

#[handler]
async fn stream_limit(req: &mut Request) {
    req.limit_body(8);
}

#[tokio::test]
async fn oversized_body_does_not_execute_handler_with_defaults() {
    let service = Service::new(Router::new().hoop(limit).post(body_data));
    for (content_type, body) in [
        ("application/json", r#"{"value":"too large"}"#),
        ("application/x-www-form-urlencoded", "value=too+large"),
    ] {
        let response = TestClient::post("http://localhost/")
            .add_header("content-type", content_type, true)
            .body(body)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::PAYLOAD_TOO_LARGE));
    }
}

#[tokio::test]
async fn streaming_body_limit_still_returns_payload_too_large() {
    let response = TestClient::post("http://localhost/")
        .add_header("content-type", "application/json", true)
        .body(r#"{"value":"too large"}"#)
        .send(Router::new().hoop(stream_limit).post(body_data))
        .await;
    assert_eq!(response.status_code, Some(StatusCode::PAYLOAD_TOO_LARGE));
}

#[tokio::test]
async fn malformed_multipart_does_not_execute_handler_with_defaults() {
    let response = TestClient::post("http://localhost/")
        .add_header("content-type", "multipart/form-data", true)
        .body("missing boundary")
        .send(Router::new().post(body_data))
        .await;
    assert_eq!(response.status_code, Some(StatusCode::BAD_REQUEST));
}

#[tokio::test]
async fn failed_body_read_does_not_execute_handler_with_defaults() {
    let service = Service::new(Router::new().post(body_data));
    for content_type in ["application/json", "application/x-www-form-urlencoded"] {
        let error: salvo_core::BoxedError = std::io::Error::other("disconnected").into();
        let stream =
            futures_util::stream::iter([Err::<hyper::body::Frame<bytes::Bytes>, _>(error)]);
        let body = salvo_core::http::ReqBody::Boxed {
            inner: Box::pin(http_body_util::StreamBody::new(stream)),
            fuse_config: None,
        };
        let response = TestClient::post("http://localhost/")
            .add_header("content-type", content_type, true)
            .body(body)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::BAD_REQUEST));
    }
}

#[tokio::test]
async fn successfully_parsed_body_still_supports_defaults() {
    let service = Service::new(Router::new().post(body_data));
    for (content_type, body, expected) in [
        ("application/json", "{}", "executed:"),
        ("application/json", r#"{"value":"ok"}"#, "executed:ok"),
        ("application/x-www-form-urlencoded", "", "executed:"),
        (
            "application/x-www-form-urlencoded",
            "value=ok",
            "executed:ok",
        ),
    ] {
        let mut response = TestClient::post("http://localhost/")
            .add_header("content-type", content_type, true)
            .body(body)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), expected);
    }
}
