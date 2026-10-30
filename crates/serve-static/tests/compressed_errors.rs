//! Compressed file metadata must not label a generated error page as compressed.

use salvo_core::fs::NamedFile;
use salvo_core::http::StatusCode;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Request, Response, Router, Service, handler};
use salvo_serve_static::{StaticDir, StaticFile};

async fn check_errors(service: &Service, path: &str) {
    for (name, value, status) in [
        ("if-match", "\"missing\"", StatusCode::PRECONDITION_FAILED),
        ("range", "bytes=999999-", StatusCode::RANGE_NOT_SATISFIABLE),
    ] {
        let mut response = TestClient::get(format!("http://localhost/{path}"))
            .add_header("accept-encoding", "gzip", true)
            .add_header(name, value, true)
            .send(service)
            .await;
        assert_eq!(response.status_code, Some(status));
        assert!(
            response.headers().get("content-encoding").is_none(),
            "{status} error pages are not gzip streams"
        );
        assert!(!response.take_string().await.unwrap().is_empty());
    }

    let response = TestClient::get(format!("http://localhost/{path}"))
        .add_header("accept-encoding", "gzip", true)
        .send(service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(response.headers()["content-encoding"], "gzip");
    let response = TestClient::head(format!("http://localhost/{path}"))
        .add_header("accept-encoding", "gzip", true)
        .send(service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(response.headers()["content-encoding"], "gzip");
    let response = TestClient::get(format!("http://localhost/{path}"))
        .add_header("accept-encoding", "gzip", true)
        .add_header("range", "bytes=0-1", true)
        .send(service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::PARTIAL_CONTENT));
    assert_eq!(response.headers()["content-encoding"], "gzip");
    let etag = response.headers()["etag"].clone();
    let response = TestClient::get(format!("http://localhost/{path}"))
        .add_header("accept-encoding", "gzip", true)
        .add_header("if-none-match", etag, true)
        .send(service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::NOT_MODIFIED));
    assert_eq!(response.headers()["content-encoding"], "gzip");
}

#[tokio::test]
async fn sidecar_errors_are_readable() {
    let service = Service::new(Router::with_path("{*path}").goal(StaticDir::new(["test/static"])));
    check_errors(&service, "test1.txt").await;
}

#[tokio::test]
async fn intrinsically_encoded_file_errors_are_readable() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("asset.svgz");
    std::fs::write(&path, include_bytes!("../test/static/test1.txt.gz")).unwrap();
    let service = Service::new(Router::with_path("asset.svgz").goal(StaticFile::new(path)));
    check_errors(&service, "asset.svgz").await;
}

#[handler]
async fn encoded_error(req: &mut Request, res: &mut Response) {
    res.headers_mut()
        .insert("content-encoding", "gzip".parse().unwrap());
    res.write_body(include_bytes!("../test/static/test1.txt.gz").as_slice())
        .unwrap();
    NamedFile::builder("test/static/test1.txt.gz")
        .content_encoding("br")
        .build()
        .await
        .unwrap()
        .send(req.headers(), res)
        .await;
}

#[tokio::test]
async fn preserves_application_encoded_error_body() {
    let service = Service::new(Router::new().goal(encoded_error));
    for (name, value, status) in [
        ("if-match", "\"missing\"", StatusCode::PRECONDITION_FAILED),
        ("range", "bytes=999999-", StatusCode::RANGE_NOT_SATISFIABLE),
    ] {
        let mut response = TestClient::get("http://localhost/")
            .add_header(name, value, true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(status));
        assert_eq!(response.headers()["content-encoding"], "gzip");
        assert_eq!(
            response.take_bytes(None).await.unwrap().as_ref(),
            include_bytes!("../test/static/test1.txt.gz")
        );
    }
}
