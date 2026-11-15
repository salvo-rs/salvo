//! Header extraction rejects values that cannot be represented as text.
#![cfg(feature = "test")]
use salvo_core::extract::Extractible;
use salvo_core::http::HeaderValue;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Depot, Router, handler};
use serde::Deserialize;

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "header")))]
struct Input {
    #[serde(rename = "x-name", alias = "x-alias")]
    name: String,
}

#[handler]
async fn input(data: Input) -> String {
    data.name
}

#[tokio::test]
async fn invalid_text_is_an_error_and_does_not_execute_the_handler() {
    let invalid = HeaderValue::from_bytes(&[0xff]).unwrap();
    let mut req = TestClient::get("http://localhost/")
        .add_header("x-name", invalid.clone(), true)
        .build();
    assert!(Input::extract(&mut req, &mut Depot::new()).await.is_err());
    let response = TestClient::get("http://localhost/")
        .add_header("x-name", invalid, true)
        .send(Router::new().get(input))
        .await;
    assert_eq!(
        response.status_code,
        Some(salvo_core::http::StatusCode::BAD_REQUEST)
    );
}

#[tokio::test]
async fn empty_and_ascii_header_values_are_preserved() {
    for value in ["", "hello"] {
        let mut response = TestClient::get("http://localhost/")
            .add_header("x-name", value, true)
            .send(Router::new().get(input))
            .await;
        assert_eq!(response.take_string().await.unwrap(), value);
    }
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[allow(dead_code)]
struct OptionalInput {
    #[serde(default, rename = "x-name")]
    #[salvo(extract(source(from = "header"), source(from = "query")))]
    name: Option<String>,
}

#[tokio::test]
async fn invalid_text_is_not_missing_and_aliases_are_checked() {
    let invalid = HeaderValue::from_bytes(&[0xff]).unwrap();
    let mut req = TestClient::get("http://localhost/?x-name=fallback")
        .add_header("x-name", invalid.clone(), true)
        .build();
    assert!(
        OptionalInput::extract(&mut req, &mut Depot::new())
            .await
            .is_err()
    );
    let mut req = TestClient::get("http://localhost/")
        .add_header("x-alias", invalid, true)
        .build();
    assert!(Input::extract(&mut req, &mut Depot::new()).await.is_err());
}
