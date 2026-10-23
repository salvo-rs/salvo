#![cfg(feature = "test")]
#![allow(missing_docs, clippy::unwrap_used)]

use std::io::Cursor;
use std::time::{Duration, UNIX_EPOCH};

use salvo_core::http::header::{IF_MATCH, IF_NONE_MATCH, IF_UNMODIFIED_SINCE};
use salvo_core::http::{HeaderMap, Response, StatusCode};
use salvo_core::test::ResponseExt;
use salvo_core::writing::ReadSeeker;

#[tokio::test]
async fn read_seeker_obeys_precondition_precedence() {
    for (if_match, if_none_match, old_date, expected) in [
        (None, None, false, StatusCode::OK),
        (None, None, true, StatusCode::PRECONDITION_FAILED),
        (Some("\"current\""), None, true, StatusCode::OK),
        (Some("*"), None, true, StatusCode::OK),
        (Some("\"old\""), None, true, StatusCode::PRECONDITION_FAILED),
        (
            Some("W/\"current\""),
            None,
            true,
            StatusCode::PRECONDITION_FAILED,
        ),
        (
            Some("\"current\""),
            Some("\"current\""),
            true,
            StatusCode::NOT_MODIFIED,
        ),
        (
            Some("\"old\""),
            Some("\"current\""),
            true,
            StatusCode::PRECONDITION_FAILED,
        ),
    ] {
        let mut headers = HeaderMap::new();
        if let Some(value) = if_match {
            headers.insert(IF_MATCH, value.parse().unwrap());
        }
        if let Some(value) = if_none_match {
            headers.insert(IF_NONE_MATCH, value.parse().unwrap());
        }
        if old_date {
            headers.insert(
                IF_UNMODIFIED_SINCE,
                "Thu, 01 Jan 1970 00:00:00 GMT".parse().unwrap(),
            );
        }
        let mut response = Response::new();
        ReadSeeker::new(Cursor::new(b"content"), 7)
            .etag("\"current\"".parse().unwrap())
            .last_modified(UNIX_EPOCH + Duration::from_secs(100))
            .send(&headers, &mut response)
            .await;
        assert_eq!(response.status_code, Some(expected), "{headers:?}");
        let body = response.take_bytes(None).await.unwrap();
        assert_eq!(
            body.as_ref(),
            if expected == StatusCode::OK {
                &b"content"[..]
            } else {
                &b""[..]
            }
        );
    }
}

#[tokio::test]
async fn if_match_wildcard_takes_precedence_without_an_etag() {
    let mut headers = HeaderMap::new();
    headers.insert(IF_MATCH, "*".parse().unwrap());
    headers.insert(
        IF_UNMODIFIED_SINCE,
        "Thu, 01 Jan 1970 00:00:00 GMT".parse().unwrap(),
    );
    let mut response = Response::new();
    ReadSeeker::new(Cursor::new(b"content"), 7)
        .last_modified(UNIX_EPOCH + Duration::from_secs(100))
        .send(&headers, &mut response)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(
        response.take_bytes(None).await.unwrap().as_ref(),
        b"content"
    );
}
