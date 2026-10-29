//! Bracketed query lists preserve token text and ignore surrounding whitespace.

#![cfg(feature = "test")]

use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Router, Service, Writer, handler};
use serde::Deserialize;

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Numbers {
    values: Vec<i32>,
}

#[handler]
async fn numbers(data: Numbers) -> String {
    format!("{:?}", data.values)
}

#[tokio::test]
async fn surrounding_list_whitespace_does_not_change_numeric_values() {
    let service = Service::new(Router::new().get(numbers));
    for value in ["[1,2]", "[ 1 , 2 ]", "[\t1\t,\n2\r\n]", "  [1, 2]  "] {
        let mut response = TestClient::get("http://localhost/")
            .query("values", value)
            .send(&service)
            .await;
        assert_eq!(
            response.status_code,
            Some(salvo_core::http::StatusCode::OK),
            "{value:?}"
        );
        assert_eq!(response.take_string().await.unwrap(), "[1, 2]", "{value:?}");
    }
}

#[derive(Debug, Deserialize, salvo_core::macros::Extractible)]
#[salvo(extract(default_source(from = "query")))]
struct Words {
    values: Vec<String>,
}

#[handler]
async fn words(data: Words) -> String {
    serde_json::to_string(&data.values).unwrap()
}

#[tokio::test]
async fn opposite_quotes_and_internal_spaces_are_part_of_string_values() {
    let service = Service::new(Router::new().get(words));
    for value in [
        r#"["O'Reilly", "two words"]"#,
        r#"['O"Reilly', 'two words']"#,
    ] {
        let mut response = TestClient::get("http://localhost/")
            .query("values", value)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(salvo_core::http::StatusCode::OK));
        let actual: Vec<String> =
            serde_json::from_str(&response.take_string().await.unwrap()).unwrap();
        let expected = if value.starts_with("[\"") {
            "O'Reilly"
        } else {
            "O\"Reilly"
        };
        assert_eq!(actual, [expected, "two words"], "{value}");
    }
}

#[tokio::test]
async fn list_delimiters_and_repeated_query_values_keep_existing_behavior() {
    let service = Service::new(Router::new().get(words));
    for (value, expected) in [
        (
            r#"["one,two", "", "say\"hi"]"#,
            vec!["one,two", "", r#"say\"hi"#],
        ),
        ("[plain text, another ]", vec!["plain text", "another"]),
        (r#"["中文", "😀"]"#, vec!["中文", "😀"]),
    ] {
        let mut response = TestClient::get("http://localhost/")
            .query("values", value)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(salvo_core::http::StatusCode::OK));
        let actual: Vec<String> =
            serde_json::from_str(&response.take_string().await.unwrap()).unwrap();
        assert_eq!(actual, expected);
    }
    let mut response = TestClient::get("http://localhost/")
        .query("values", "[literal]")
        .query("values", "second")
        .send(&service)
        .await;
    let actual: Vec<String> = serde_json::from_str(&response.take_string().await.unwrap()).unwrap();
    assert_eq!(actual, ["[literal]", "second"]);
}
