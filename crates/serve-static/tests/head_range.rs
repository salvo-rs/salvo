//! Static handlers must ignore Range when responding to HEAD.

use salvo_core::http::StatusCode;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Router, Service};
use salvo_serve_static::{StaticDir, StaticFile};

async fn check_head_range(service: &Service, check_if_match: bool) {
    let mut original = TestClient::get("http://localhost/test1.txt")
        .send(service)
        .await;
    let etag = original.headers()["etag"].clone();
    let body = original.take_bytes(None).await.unwrap();
    for range in ["bytes=1-", "bytes=999-", "invalid"] {
        let response = TestClient::head("http://localhost/test1.txt")
            .add_header("range", range, true)
            .send(service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK), "{range}");
        assert_eq!(response.headers()["content-length"], body.len().to_string());
        assert!(!response.headers().contains_key("content-range"));
        assert!(response.body.is_none());
        let response = TestClient::head("http://localhost/test1.txt")
            .add_header("range", range, true)
            .add_header("if-none-match", etag.clone(), true)
            .send(service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NOT_MODIFIED));
        assert!(response.body.is_none());
        if check_if_match {
            let response = TestClient::head("http://localhost/test1.txt")
                .add_header("range", range, true)
                .add_header("if-match", "\"old\"", true)
                .send(service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::PRECONDITION_FAILED));
            assert!(response.body.is_none());
        }
    }
    let mut response = TestClient::get("http://localhost/test1.txt")
        .add_header("range", "bytes=1-", true)
        .send(service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::PARTIAL_CONTENT));
    assert_eq!(response.take_bytes(None).await.unwrap(), body.slice(1..));
}

#[tokio::test]
async fn static_file_head_ignores_range() {
    let service =
        Service::new(Router::with_path("test1.txt").goal(StaticFile::new("test/static/test1.txt")));
    check_head_range(&service, true).await;
}

#[tokio::test]
async fn static_dir_head_ignores_range() {
    let service = Service::new(Router::with_path("{*path}").goal(StaticDir::new(["test/static"])));
    check_head_range(&service, true).await;
}

#[cfg(feature = "embed")]
#[tokio::test]
async fn static_embed_head_ignores_range() {
    #[derive(rust_embed::RustEmbed)]
    #[folder = "test/static/"]
    struct Assets;
    let service = Service::new(
        Router::with_path("{*path}").goal(salvo_serve_static::StaticEmbed::<Assets>::new()),
    );
    check_head_range(&service, false).await;
}
