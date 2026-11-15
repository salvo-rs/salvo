//! A final filename must never serve as an upload ID for deleting its payload.

use salvo_core::Service;
use salvo_core::http::StatusCode;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_tus::Tus;

#[tokio::test]
async fn final_filename_cannot_delete_or_replace_another_upload() {
    let directory = tempfile::tempdir().unwrap();
    let service = Service::new(
        Tus::new()
            .path("/uploads")
            .storage_root(directory.path())
            .into_router(),
    );
    let created = TestClient::post("http://localhost/uploads")
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("upload-length", "6", true)
        .add_header("upload-metadata", "filename a25vd24uYmlu", true) // known.bin
        .add_header("content-type", "application/offset+octet-stream", true)
        .body("victim")
        .send(&service)
        .await;
    assert_eq!(created.status_code, Some(StatusCode::CREATED));
    let location = created.headers()["location"].to_str().unwrap();
    let url = if location.starts_with('/') {
        format!("http://localhost{location}")
    } else {
        location.to_owned()
    };
    let payload_path = directory.path().join("data/known.bin");
    assert_eq!(std::fs::read(&payload_path).unwrap(), b"victim");

    let deleted = TestClient::delete("http://localhost/uploads/known")
        .add_header("tus-resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert_eq!(deleted.status_code, Some(StatusCode::NOT_FOUND));
    assert_eq!(std::fs::read(&payload_path).unwrap(), b"victim");

    // An attacker choosing the filename stem as their own ID must also fail
    // to claim the victim's internal-looking payload path.
    let attacker = Service::new(
        Tus::new()
            .path("/uploads")
            .storage_root(directory.path())
            .upload_id_naming_function(|_, _| async { Ok("known".to_owned()) })
            .into_router(),
    );
    let replaced = TestClient::post("http://localhost/uploads")
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("upload-length", "6", true)
        .add_header("content-type", "application/offset+octet-stream", true)
        .body("attack")
        .send(&attacker)
        .await;
    assert_eq!(
        replaced.status_code,
        Some(StatusCode::INTERNAL_SERVER_ERROR)
    );
    let unknown = TestClient::get("http://localhost/uploads/known")
        .add_header("tus-resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert_eq!(unknown.status_code, Some(StatusCode::NOT_FOUND));
    let mut downloaded = TestClient::get(&url)
        .add_header("tus-resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert_eq!(downloaded.status_code, Some(StatusCode::OK));
    assert_eq!(downloaded.take_bytes(None).await.unwrap(), "victim");

    let deleted = TestClient::delete(url)
        .add_header("tus-resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert_eq!(deleted.status_code, Some(StatusCode::NO_CONTENT));
    assert!(!payload_path.exists());
}
