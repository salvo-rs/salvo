#![cfg(feature = "test")]

use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Depot, Router, Service, Writer, handler};
use serde::Deserialize;

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "body")))]
struct BodyData<'a> {
    name: &'a str,
    #[serde(alias = "编号")]
    number: u32,
    #[serde(rename = "quoted\"key")]
    quoted: bool,
}

#[handler]
async fn read_body(data: BodyData<'_>) -> String {
    format!("{}:{}:{}", data.name, data.number, data.quoted)
}

#[tokio::test]
async fn escaped_object_keys_match_plain_keys_and_aliases() {
    let service = Service::new(Router::new().post(read_body));
    for body in [
        r#"{"name":"borrowed", "number":7, "quoted\"key":true}"#,
        r#"{"na\u006de":"borrowed", "\u7f16\u53f7":7, "quoted\u0022key":true}"#,
    ] {
        let parsed: BodyData<'_> = serde_json::from_str(body).unwrap();
        assert_eq!(parsed.name, "borrowed");
        let mut response = TestClient::post("http://localhost/")
            .add_header("content-type", "application/json", true)
            .body(body)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(salvo_core::http::StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "borrowed:7:true");
    }
}

#[tokio::test]
async fn unrelated_escaped_keys_do_not_change_object_extraction() {
    #[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
    #[salvo(extract(default_source(from = "body")))]
    struct Named<'a> {
        name: &'a str,
    }
    let mut req = TestClient::post("http://localhost/")
        .add_header("content-type", "application/json", true)
        .body(r#"{"name":"borrowed", "unused\u0020key":null}"#)
        .build();
    let mut depot = Depot::new();
    let data: Named<'_> = req.extract(&mut depot).await.unwrap();
    assert_eq!(data.name, "borrowed");
}
