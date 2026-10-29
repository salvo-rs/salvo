//! Upload identities remain reserved after a completed payload is renamed.

use salvo_core::Service;
use salvo_core::http::StatusCode;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_tus::Tus;

fn service(directory: &std::path::Path) -> Service {
    Service::new(
        Tus::new()
            .path("/uploads")
            .storage_root(directory)
            .upload_id_naming_function(|_, _| async { Ok("same-id".to_owned()) })
            .into_router(),
    )
}

#[tokio::test]
async fn renamed_upload_cannot_be_replaced_by_a_duplicate_id() {
    let directory = tempfile::tempdir().unwrap();
    let first_service = service(directory.path());
    let created = TestClient::post("http://localhost/uploads")
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("upload-length", "3", true)
        .add_header("upload-metadata", "filename ZG9jdW1lbnQudHh0", true)
        .add_header("content-type", "application/offset+octet-stream", true)
        .body("abc")
        .send(&first_service)
        .await;
    assert_eq!(created.status_code, Some(StatusCode::CREATED));
    assert!(directory.path().join("data/document.txt").exists());
    assert!(!directory.path().join("data/same-id.bin").exists());
    let metadata = std::fs::read(directory.path().join("metadata/same-id.json")).unwrap();

    // Reopening the store must preserve the identity reservation. Use separate
    // services (and lockers) to exercise storage-level collision protection too.
    drop(first_service);
    let reopened = service(directory.path());
    let concurrent = service(directory.path());
    async fn duplicate(service: &Service) -> salvo_core::Response {
        TestClient::post("http://localhost/uploads")
            .add_header("tus-resumable", "1.0.0", true)
            .add_header("upload-length", "10", true)
            .send(service)
            .await
    }
    let (first, second) = tokio::join!(duplicate(&reopened), duplicate(&concurrent));
    for response in [first, second] {
        assert!(
            response
                .status_code
                .is_some_and(|status| !status.is_success()),
            "duplicate creation unexpectedly succeeded: {:?}",
            response.status_code
        );
    }
    assert_eq!(
        std::fs::read(directory.path().join("metadata/same-id.json")).unwrap(),
        metadata
    );
    assert!(!directory.path().join("data/same-id.bin").exists());
    let head = TestClient::head("http://localhost/uploads/same-id")
        .add_header("tus-resumable", "1.0.0", true)
        .send(&reopened)
        .await;
    assert_eq!(head.headers()["upload-length"], "3");
    assert_eq!(head.headers()["upload-offset"], "3");
    let mut downloaded = TestClient::get("http://localhost/uploads/same-id")
        .add_header("tus-resumable", "1.0.0", true)
        .send(&reopened)
        .await;
    assert_eq!(downloaded.take_bytes(None).await.unwrap(), "abc");
}

#[tokio::test]
async fn unreadable_existing_metadata_is_not_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("metadata")).unwrap();
    let metadata = directory.path().join("metadata/same-id.json");
    std::fs::write(&metadata, "damaged existing record").unwrap();
    let service = service(directory.path());
    let rejected = TestClient::post("http://localhost/uploads")
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("upload-length", "10", true)
        .send(&service)
        .await;
    assert_eq!(
        rejected.status_code,
        Some(StatusCode::INTERNAL_SERVER_ERROR)
    );
    assert_eq!(
        std::fs::read_to_string(metadata).unwrap(),
        "damaged existing record"
    );
    assert!(!directory.path().join("data/same-id.bin").exists());
}
