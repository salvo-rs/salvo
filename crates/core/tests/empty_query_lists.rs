//! Explicit empty bracket lists deserialize as empty typed request sequences.

#![cfg(feature = "test")]

use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Router, Service, Writer, handler};
use serde::Deserialize;

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Lists {
    numbers: Vec<i32>,
    words: Vec<String>,
    optional: Option<Vec<i32>>,
    fixed: [i32; 0],
}

#[handler]
async fn lists(data: Lists) -> String {
    assert!(data.numbers.is_empty());
    assert!(data.words.is_empty());
    assert_eq!(data.optional, Some(Vec::new()));
    assert_eq!(data.fixed, [0_i32; 0]);
    "empty".into()
}

#[tokio::test]
async fn explicit_empty_brackets_are_empty_typed_collections() {
    let service = Service::new(Router::new().get(lists));
    for value in ["[]", "[ ]", " \t[ \n\t ]\r "] {
        let mut response = TestClient::get("http://localhost/")
            .query("numbers", value)
            .query("words", value)
            .query("optional", value)
            .query("fixed", value)
            .send(&service)
            .await;
        assert_eq!(
            response.status_code,
            Some(salvo_core::http::StatusCode::OK),
            "{value:?}"
        );
        assert_eq!(response.take_string().await.unwrap(), "empty");
    }
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Words {
    values: Vec<String>,
    literal: String,
}

#[handler]
async fn words(data: Words) -> String {
    assert_eq!(data.literal, "[]");
    serde_json::to_string(&data.values).unwrap()
}

#[tokio::test]
async fn empty_string_members_and_scalar_literals_remain_distinct() {
    let service = Service::new(Router::new().get(words));
    for (values, expected) in [
        (vec!["[]"], Vec::<String>::new()),
        (vec![r#"[""]"#], vec!["".into()]),
        (vec![r#"["[]"]"#], vec!["[]".into()]),
        (vec!["[]", "second"], vec!["[]".into(), "second".into()]),
        (vec!["[literal]"], vec!["[literal]".into()]),
    ] {
        let mut request = TestClient::get("http://localhost/").query("literal", "[]");
        for value in values {
            request = request.query("values", value);
        }
        let mut response = request.send(&service).await;
        assert_eq!(response.status_code, Some(salvo_core::http::StatusCode::OK));
        let actual: Vec<String> =
            serde_json::from_str(&response.take_string().await.unwrap()).unwrap();
        assert_eq!(actual, expected);
    }
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "header")))]
struct HeaderLists {
    values: Vec<i32>,
    literal: String,
}

#[handler]
async fn header_lists(data: HeaderLists) -> String {
    assert!(data.values.is_empty());
    data.literal
}

#[tokio::test]
async fn shared_flat_header_parser_also_accepts_empty_collections() {
    let service = Service::new(Router::new().get(header_lists));
    for value in ["[]", "[ \t ]"] {
        let mut response = TestClient::get("http://localhost/")
            .add_header("values", value, true)
            .add_header("literal", "[]", true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(salvo_core::http::StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "[]");
    }
}
