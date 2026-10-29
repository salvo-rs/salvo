#![cfg(feature = "gzip")]
#![allow(missing_docs, clippy::unwrap_used)]

use salvo_compression::Compression;
use salvo_core::http::header::{CACHE_CONTROL, CONTENT_ENCODING, CONTENT_LENGTH, ETAG, VARY};
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};

#[handler]
async fn representation(req: &mut Request, res: &mut Response) {
    // Model a response forwarded from an upstream origin without modifying its
    // representation metadata before the outer compression middleware runs.
    for value in req.headers().get_all("x-origin-cache-control") {
        res.headers_mut().append(CACHE_CONTROL, value.clone());
    }
    res.add_header(CONTENT_LENGTH, "4096", true).unwrap();
    res.add_header(ETAG, "\"original\"", true).unwrap();
    res.render("a".repeat(4096));
    match req.uri().path() {
        "/head" | "/not-modified" => {
            res.take_body();
            if req.uri().path() == "/not-modified" {
                res.status_code(StatusCode::NOT_MODIFIED);
            }
        }
        "/stream" => res.stream(futures_util::stream::once(async {
            Ok::<_, std::io::Error>("a".repeat(4096))
        })),
        _ => {}
    }
}

#[tokio::test]
async fn no_transform_preserves_response_representation() {
    let service = Service::new(Router::new().hoop(Compression::new()).get(representation));
    for headers in [
        vec!["no-transform"],
        vec!["public, max-age=60, No-TrAnSfOrM"],
        vec!["max-age=60", "no-transform"],
        vec!["extension=\"value, no-transform\", no-transform"],
        vec!["extension=\"escaped\\\"quote\", no-transform"],
    ] {
        let mut request =
            TestClient::get("http://localhost/").add_header("accept-encoding", "gzip", true);
        for value in headers {
            request = request.add_header("x-origin-cache-control", value, false);
        }
        let mut response = request.send(&service).await;
        assert!(response.headers().get(CONTENT_ENCODING).is_none());
        assert_eq!(response.headers()[CONTENT_LENGTH], "4096");
        assert_eq!(response.headers()[ETAG], "\"original\"");
        assert!(response.headers().get(VARY).is_none());
        assert_eq!(response.take_bytes(None).await.unwrap(), "a".repeat(4096));
    }
}

#[tokio::test]
async fn quoted_extension_does_not_disable_compression() {
    let service = Service::new(Router::new().hoop(Compression::new()).get(representation));
    let response = TestClient::get("http://localhost/")
        .add_header("accept-encoding", "gzip", true)
        .add_header(
            "x-origin-cache-control",
            "extension=\"value, no-transform\"",
            true,
        )
        .send(&service)
        .await;
    assert_eq!(response.headers()[CONTENT_ENCODING], "gzip");
}

#[tokio::test]
async fn no_transform_request_preserves_response_representation() {
    let service = Service::new(Router::new().hoop(Compression::new()).get(representation));
    let mut response = TestClient::get("http://localhost/")
        .add_header("accept-encoding", "gzip", true)
        .add_header(CACHE_CONTROL, "max-age=0", true)
        .add_header(CACHE_CONTROL, "No-Transform", false)
        .send(&service)
        .await;
    assert!(response.headers().get(CONTENT_ENCODING).is_none());
    assert_eq!(response.headers()[CONTENT_LENGTH], "4096");
    assert_eq!(response.headers()[ETAG], "\"original\"");
    assert_eq!(response.take_bytes(None).await.unwrap(), "a".repeat(4096));
}

#[tokio::test]
async fn no_transform_also_preserves_stream_head_and_not_modified_metadata() {
    let service = Service::new(
        Router::with_path("{kind}")
            .hoop(Compression::new())
            .goal(representation),
    );
    for path in ["stream", "head", "not-modified"] {
        let url = format!("http://localhost/{path}");
        let request = if path == "head" {
            TestClient::head(url)
        } else {
            TestClient::get(url)
        };
        let mut response = request
            .add_header("accept-encoding", "gzip", true)
            .add_header("x-origin-cache-control", "no-transform", true)
            .send(&service)
            .await;
        assert!(response.headers().get(CONTENT_ENCODING).is_none());
        assert_eq!(response.headers()[ETAG], "\"original\"");
        assert!(response.headers().get(VARY).is_none());
        if path == "stream" {
            assert_eq!(response.take_bytes(None).await.unwrap(), "a".repeat(4096));
        } else {
            assert!(response.take_bytes(None).await.unwrap().is_empty());
        }
    }
}
