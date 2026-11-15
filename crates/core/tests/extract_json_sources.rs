//! Explicit JSON parsers apply to textual query and header values.
#![cfg(feature = "test")]
use salvo_core::extract::Extractible;
use salvo_core::test::TestClient;
use salvo_core::{Depot, Request};
use serde::Deserialize;

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct Text<'a> {
    #[salvo(extract(
        source(from = "query", parse = "json"),
        source(from = "header", parse = "json")
    ))]
    value: &'a str,
}

fn requests(query: &str, header: &str) -> [Request; 2] {
    [
        TestClient::get(format!("http://localhost/?value={query}")).build(),
        TestClient::get("http://localhost/")
            .add_header("value", header, true)
            .build(),
    ]
}

#[tokio::test]
async fn json_strings_are_decoded_and_can_borrow_the_request() {
    for mut req in requests("%22hello%22", r#""hello""#) {
        assert_eq!(
            Text::extract(&mut req, &mut Depot::new())
                .await
                .unwrap()
                .value,
            "hello"
        );
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct Object {
    name: String,
    count: u32,
}
#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
struct Nested {
    #[salvo(extract(
        source(from = "query", parse = "json"),
        source(from = "header", parse = "json")
    ))]
    value: Object,
}

#[tokio::test]
async fn structured_values_and_json_errors_use_the_selected_parser() {
    for mut req in requests(
        "%7B%22name%22%3A%22a%22%2C%22count%22%3A2%7D",
        r#"{"name":"a","count":2}"#,
    ) {
        assert_eq!(
            Nested::extract(&mut req, &mut Depot::new())
                .await
                .unwrap()
                .value,
            Object {
                name: "a".into(),
                count: 2
            }
        );
    }
    for (query, header) in [("hello", "hello"), ("%22hello%22x", r#""hello"x"#)] {
        for mut req in requests(query, header) {
            assert!(Text::extract(&mut req, &mut Depot::new()).await.is_err());
        }
    }
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Flat {
    value: String,
}

#[tokio::test]
async fn automatic_query_parser_keeps_literal_quotes() {
    let mut req = TestClient::get("http://localhost/?value=%22hello%22").build();
    assert_eq!(
        Flat::extract(&mut req, &mut Depot::new())
            .await
            .unwrap()
            .value,
        r#""hello""#
    );
}

#[tokio::test]
async fn repeated_json_documents_are_rejected_instead_of_losing_values() {
    let mut query = TestClient::get("http://localhost/?value=%22a%22&value=%22b%22").build();
    assert!(Text::extract(&mut query, &mut Depot::new()).await.is_err());
    let mut header = TestClient::get("http://localhost/")
        .add_header("value", r#""a""#, false)
        .add_header("value", r#""b""#, false)
        .build();
    assert!(Text::extract(&mut header, &mut Depot::new()).await.is_err());
}
