//! HTTP response semantics are finalized before selecting a transport.
#![cfg(feature = "test")]

use std::io::Cursor;

use salvo_core::http::{HeaderMap, StatusCode};
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::writing::ReadSeeker;

#[tokio::test]
async fn head_runs_the_catcher_but_suppresses_its_body() {
    #[handler]
    async fn fail(res: &mut Response) {
        res.status_code(StatusCode::UNAUTHORIZED);
    }
    #[handler]
    async fn error_page(res: &mut Response) {
        res.add_header("www-authenticate", "Bearer", true).unwrap();
        res.add_header("x-error-policy", "applied", true).unwrap();
        res.render("authenticate");
    }
    let service = Service::new(Router::new().goal(fail))
        .catcher(salvo_core::catcher::Catcher::new(error_page));
    let mut get = TestClient::get("http://localhost/").send(&service).await;
    let head = TestClient::head("http://localhost/").send(&service).await;
    assert_eq!(get.status_code, head.status_code);
    for header in ["www-authenticate", "x-error-policy", "content-type"] {
        assert_eq!(get.headers().get(header), head.headers().get(header));
    }
    assert_eq!(get.take_string().await.unwrap(), "authenticate");
    assert!(head.body.is_none());
}

#[tokio::test]
async fn bodyless_statuses_discard_content_and_incompatible_lengths() {
    #[handler]
    async fn body(req: &mut Request, res: &mut Response) {
        res.status_code(StatusCode::from_u16(req.query("status").unwrap()).unwrap());
        res.add_header("content-length", "5", true).unwrap();
        res.render("hello");
    }
    let service = Service::new(Router::new().goal(body));
    for status in [204, 205, 304] {
        let response = TestClient::get(format!("http://localhost/?status={status}"))
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap().as_u16(), status);
        assert!(response.body.is_none());
        if status == 204 {
            assert!(!response.headers().contains_key("content-length"));
        }
        if status == 205 {
            assert_eq!(response.headers()["content-length"], "0");
        }
        if status == 304 {
            assert_eq!(response.headers()["content-length"], "5");
        }
    }
}

#[cfg(all(feature = "catcher-json", feature = "catcher-html"))]
#[tokio::test]
async fn error_format_negotiation_respects_quality_specificity_and_all_field_lines() {
    for (accept, expected) in [
        ("text/html;q=0, application/json;q=1", "application/json"),
        ("image/png, application/json;q=0.5", "application/json"),
        (
            "text/*;q=0.9,text/html;q=0,text/xml;q=0,*/*;q=0.5",
            "text/plain",
        ),
        ("application/json;q=0,*/*;q=0.5", "text/html"),
        (
            "application/json;charset=utf-8;q=0.8,text/html;q=0.1",
            "application/json; charset=utf-8",
        ),
        ("text/html;q=NaN,application/json;q=0.5", "application/json"),
        (
            "application/json;profile=\"x,y\";q=1,text/plain;q=0.5",
            "text/plain",
        ),
    ] {
        let response = TestClient::get("http://localhost/")
            .add_header("accept", accept, true)
            .send(Router::new())
            .await;
        assert_eq!(response.headers()["content-type"], expected, "{accept}");
        assert!(
            response.headers()["vary"]
                .to_str()
                .unwrap()
                .contains("accept")
        );
    }
    let response = TestClient::get("http://localhost/")
        .add_header("accept", "text/html;q=0.1", true)
        .add_header("accept", "application/json;q=0.9", false)
        .send(Router::new())
        .await;
    assert_eq!(response.headers()["content-type"], "application/json");
}

#[tokio::test]
async fn full_read_seeker_responses_always_start_at_zero() {
    for if_range in [false, true] {
        let mut reader = Cursor::new(b"abcdef");
        reader.set_position(2);
        let mut headers = HeaderMap::new();
        if if_range {
            headers.insert("range", "bytes=0-1".parse().unwrap());
            headers.insert("if-range", "\"old\"".parse().unwrap());
        }
        let mut response = Response::new();
        ReadSeeker::new(reader, 6)
            .etag("\"current\"".parse().unwrap())
            .send(&headers, &mut response)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.headers()["content-length"], "6");
        assert_eq!(response.take_string().await.unwrap(), "abcdef");
    }
}

#[tokio::test]
async fn not_modified_file_writers_discard_a_prepared_body() {
    let mut headers = HeaderMap::new();
    headers.insert("if-none-match", "*".parse().unwrap());
    let mut response = Response::new();
    response.render("old-body");
    ReadSeeker::new(Cursor::new(b"abc"), 3)
        .send(&headers, &mut response)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::NOT_MODIFIED));
    assert!(response.body.is_none());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("file.txt");
    std::fs::write(&path, "abc").unwrap();
    let mut response = Response::new();
    response.render("old-body");
    salvo_core::fs::NamedFile::open(path)
        .await
        .unwrap()
        .send(&headers, &mut response)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::NOT_MODIFIED));
    assert!(response.body.is_none());
}
