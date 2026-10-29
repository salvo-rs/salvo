//! Media type tokens are case insensitive in both tus upload entry points.

use salvo_core::Service;
use salvo_core::http::StatusCode;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_tus::Tus;

const CONTENT_TYPES: [&str; 4] = [
    "application/offset+octet-stream",
    "APPLICATION/OFFSET+OCTET-STREAM",
    "Application/Offset+Octet-Stream",
    " \tApplication/Offset+Octet-Stream\t ",
];

#[tokio::test]
async fn creation_with_upload_accepts_media_type_case_variants() {
    let directory = tempfile::tempdir().unwrap();
    let service = Service::new(
        Tus::new()
            .path("/uploads")
            .storage_root(directory.path())
            .into_router(),
    );
    for content_type in CONTENT_TYPES {
        let created = TestClient::post("http://localhost/uploads")
            .add_header("tus-resumable", "1.0.0", true)
            .add_header("upload-length", "3", true)
            .add_header("content-type", content_type, true)
            .body("abc")
            .send(&service)
            .await;
        assert_eq!(
            created.status_code,
            Some(StatusCode::CREATED),
            "{content_type}"
        );
        assert_eq!(created.headers()["upload-offset"], "3");
        let url = format!(
            "http://localhost{}",
            created.headers()["location"].to_str().unwrap()
        );
        let mut download = TestClient::get(url)
            .add_header("tus-resumable", "1.0.0", true)
            .send(&service)
            .await;
        assert_eq!(download.take_bytes(None).await.unwrap(), "abc");
    }
}

#[tokio::test]
async fn patch_accepts_media_type_case_variants() {
    let directory = tempfile::tempdir().unwrap();
    let service = Service::new(
        Tus::new()
            .path("/uploads")
            .storage_root(directory.path())
            .into_router(),
    );
    for content_type in CONTENT_TYPES {
        let created = TestClient::post("http://localhost/uploads")
            .add_header("tus-resumable", "1.0.0", true)
            .add_header("upload-length", "3", true)
            .send(&service)
            .await;
        assert_eq!(created.status_code, Some(StatusCode::CREATED));
        let url = format!(
            "http://localhost{}",
            created.headers()["location"].to_str().unwrap()
        );
        for invalid_type in [
            "text/plain",
            "application/octet-stream",
            "application/offset+octet-stream; charset=utf-8",
        ] {
            let rejected = TestClient::patch(&url)
                .add_header("tus-resumable", "1.0.0", true)
                .add_header("upload-offset", "0", true)
                .add_header("content-type", invalid_type, true)
                .body("bad")
                .send(&service)
                .await;
            assert_eq!(
                rejected.status_code,
                Some(StatusCode::UNSUPPORTED_MEDIA_TYPE)
            );
        }
        let patched = TestClient::patch(&url)
            .add_header("tus-resumable", "1.0.0", true)
            .add_header("upload-offset", "0", true)
            .add_header("content-type", content_type, true)
            .body("abc")
            .send(&service)
            .await;
        assert_eq!(
            patched.status_code,
            Some(StatusCode::NO_CONTENT),
            "{content_type}"
        );
        assert_eq!(patched.headers()["upload-offset"], "3");
        let mut download = TestClient::get(url)
            .add_header("tus-resumable", "1.0.0", true)
            .send(&service)
            .await;
        assert_eq!(download.take_bytes(None).await.unwrap(), "abc");
    }
}
