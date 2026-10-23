//! HEAD returns complete representation metadata even when Range is supplied.

#![cfg(feature = "test")]

use std::io::Cursor;

use salvo_core::fs::NamedFile;
use salvo_core::http::headers::HeaderMapExt;
use salvo_core::http::{HeaderMap, Response, StatusCode};
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::writing::ReadSeeker;
use salvo_core::{Depot, Writer};

fn assert_head(response: &Response, expected: StatusCode) {
    assert_eq!(response.status_code, Some(expected));
    assert!(response.body.is_none());
    assert!(!response.headers().contains_key("content-range"));
    if expected == StatusCode::OK {
        assert_eq!(response.headers()["content-length"], "10");
    }
}

#[tokio::test]
async fn named_file_head_ignores_range_and_preserves_preconditions() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("head.txt");
    std::fs::write(&path, b"0123456789").unwrap();
    for preload in [0, 64] {
        let file = NamedFile::open(&path).await.unwrap();
        let mut validators = HeaderMap::new();
        validators.typed_insert(file.etag().unwrap());
        let etag = validators["etag"].to_str().unwrap();
        for direct in [false, true] {
            for range in ["bytes=2-", "bytes=999-", "invalid"] {
                for (condition, expected) in [
                    (None, StatusCode::OK),
                    (Some(("if-none-match", etag)), StatusCode::NOT_MODIFIED),
                    (
                        Some(("if-match", "\"old\"")),
                        StatusCode::PRECONDITION_FAILED,
                    ),
                ] {
                    let mut req = TestClient::head("http://localhost/")
                        .add_header("range", range, true)
                        .build();
                    if let Some((name, value)) = condition {
                        req.headers_mut().insert(name, value.parse().unwrap());
                    }
                    let file = NamedFile::builder(&path)
                        .preload_threshold(preload)
                        .build()
                        .await
                        .unwrap();
                    let mut response = Response::new();
                    if direct {
                        file.send_head(req.headers(), &mut response).await;
                    } else {
                        file.write(&mut req, &mut Depot::new(), &mut response).await;
                    }
                    assert_head(&response, expected);
                }
            }
        }
        let mut req = TestClient::get("http://localhost/")
            .add_header("range", "bytes=2-", true)
            .build();
        let mut response = Response::new();
        NamedFile::builder(&path)
            .preload_threshold(preload)
            .build()
            .await
            .unwrap()
            .write(&mut req, &mut Depot::new(), &mut response)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::PARTIAL_CONTENT));
        assert_eq!(response.headers()["content-length"], "8");
        assert_eq!(response.take_string().await.unwrap(), "23456789");
    }
}

#[tokio::test]
async fn read_seeker_writer_head_ignores_range_and_preserves_preconditions() {
    for range in ["bytes=2-", "bytes=999-", "invalid"] {
        for (condition, expected) in [
            (None, StatusCode::OK),
            (Some(("if-none-match", "\"new\"")), StatusCode::NOT_MODIFIED),
            (
                Some(("if-match", "\"old\"")),
                StatusCode::PRECONDITION_FAILED,
            ),
        ] {
            let mut req = TestClient::head("http://localhost/")
                .add_header("range", range, true)
                .build();
            if let Some((name, value)) = condition {
                req.headers_mut().insert(name, value.parse().unwrap());
            }
            let mut response = Response::new();
            ReadSeeker::new(Cursor::new(b"0123456789"), 10)
                .etag("\"new\"".parse().unwrap())
                .write(&mut req, &mut Depot::new(), &mut response)
                .await;
            assert_head(&response, expected);
        }
    }
    let mut req = TestClient::get("http://localhost/")
        .add_header("range", "bytes=2-", true)
        .build();
    let mut response = Response::new();
    ReadSeeker::new(Cursor::new(b"0123456789"), 10)
        .write(&mut req, &mut Depot::new(), &mut response)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::PARTIAL_CONTENT));
    assert_eq!(response.headers()["content-length"], "8");
    assert_eq!(response.take_string().await.unwrap(), "23456789");
}
