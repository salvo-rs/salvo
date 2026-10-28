//! ReadSeeker applies conditional and range semantics for the actual request method.

#![cfg(feature = "test")]

use std::io::Cursor;
use std::time::{Duration, UNIX_EPOCH};

use salvo_core::http::{Method, Response, StatusCode};
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::writing::ReadSeeker;
use salvo_core::{Depot, Writer};

async fn send(method: Method, name: &'static str, value: &str) -> Response {
    let mut req = TestClient::get("http://localhost/")
        .add_header(name, value, true)
        .build();
    *req.method_mut() = method;
    let mut response = Response::new();
    ReadSeeker::new(Cursor::new(b"0123456789"), 10)
        .etag("\"version\"".parse().unwrap())
        .last_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
        .write(&mut req, &mut Depot::new(), &mut response)
        .await;
    response
}

#[tokio::test]
async fn non_get_methods_ignore_range_including_invalid_and_unsatisfiable_values() {
    for method in [
        Method::POST,
        Method::PUT,
        Method::PATCH,
        Method::DELETE,
        Method::OPTIONS,
    ] {
        for range in ["bytes=2-3", "bytes=999-", "invalid"] {
            let mut response = send(method.clone(), "range", range).await;
            assert_eq!(
                response.status_code,
                Some(StatusCode::OK),
                "{method}: {range}"
            );
            assert!(!response.headers().contains_key("content-range"));
            assert_eq!(response.headers()["content-length"], "10");
            assert_eq!(response.take_string().await.unwrap(), "0123456789");
        }
    }
    let mut response = send(Method::GET, "range", "bytes=2-3").await;
    assert_eq!(response.status_code, Some(StatusCode::PARTIAL_CONTENT));
    assert_eq!(response.take_string().await.unwrap(), "23");
}

#[tokio::test]
async fn matching_if_none_match_uses_method_specific_status() {
    for method in [
        Method::GET,
        Method::HEAD,
        Method::POST,
        Method::PUT,
        Method::DELETE,
    ] {
        let response = send(method.clone(), "if-none-match", "\"version\"").await;
        let expected = if method == Method::GET || method == Method::HEAD {
            StatusCode::NOT_MODIFIED
        } else {
            StatusCode::PRECONDITION_FAILED
        };
        assert_eq!(response.status_code, Some(expected), "{method}");
        assert!(response.body.is_none());
    }
}

#[tokio::test]
async fn if_modified_since_is_ignored_for_non_retrieval_methods() {
    for method in [
        Method::GET,
        Method::HEAD,
        Method::POST,
        Method::PUT,
        Method::DELETE,
    ] {
        let mut response = send(
            method.clone(),
            "if-modified-since",
            "Tue, 14 Nov 2023 22:13:21 GMT",
        )
        .await;
        if method == Method::GET || method == Method::HEAD {
            assert_eq!(response.status_code, Some(StatusCode::NOT_MODIFIED));
            assert!(response.body.is_none());
        } else {
            assert_eq!(response.status_code, Some(StatusCode::OK));
            assert_eq!(response.take_string().await.unwrap(), "0123456789");
        }
    }
}
