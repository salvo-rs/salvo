//! Missing body fields allow the next configured extraction source.
#![cfg(feature = "test")]
use salvo_core::Depot;
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;
use serde::Deserialize;

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct Input {
    #[serde(alias = "legacy")]
    #[salvo(extract(source(from = "body"), source(from = "query")))]
    id: u32,
}

#[tokio::test]
async fn missing_json_and_form_fields_fall_back_to_query() {
    for (content_type, body) in [
        ("application/json", "{}"),
        ("application/x-www-form-urlencoded", "other=9"),
    ] {
        let mut req = TestClient::post("http://localhost/?id=7")
            .add_header("content-type", content_type, true)
            .body(body)
            .build();
        assert_eq!(
            Input::extract(&mut req, &mut Depot::new())
                .await
                .unwrap()
                .id,
            7
        );
    }
}

#[tokio::test]
async fn present_body_values_and_aliases_keep_priority_and_errors() {
    for body in [r#"{"id":3}"#, r#"{"legacy":3}"#] {
        let mut req = TestClient::post("http://localhost/?id=7")
            .add_header("content-type", "application/json", true)
            .body(body)
            .build();
        assert_eq!(
            Input::extract(&mut req, &mut Depot::new())
                .await
                .unwrap()
                .id,
            3
        );
    }
    let mut req = TestClient::post("http://localhost/?id=7")
        .add_header("content-type", "application/json", true)
        .body(r#"{"id":"invalid"}"#)
        .build();
    assert!(Input::extract(&mut req, &mut Depot::new()).await.is_err());
}
