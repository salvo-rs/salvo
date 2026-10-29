//! Lossless metadata through HTTP, storage reopening, and legacy records.

use std::collections::HashMap;
use std::path::Path;

use salvo_core::Service;
use salvo_core::http::StatusCode;
use salvo_core::test::TestClient;
use salvo_tus::{DataStore, DiskStore, Metadata, Tus};
use serde_json::json;

#[tokio::test]
async fn binary_metadata_survives_post_head_and_store_reopening() {
    let directory = tempfile::tempdir().unwrap();
    let service = Service::new(
        Tus::new()
            .path("/uploads")
            .storage_root(directory.path())
            .upload_id_naming_function(|_, metadata| async move {
                let metadata = metadata.unwrap();
                assert_eq!(metadata.get("binary"), Some(&Some(vec![0xff, 0])));
                assert!(metadata.get_text("binary").is_err());
                Ok("binary-metadata".to_owned())
            })
            .into_router(),
    );
    let expected = Metadata(HashMap::from([
        ("binary".to_owned(), Some(vec![0xff, 0])),
        ("nul".to_owned(), Some(vec![0])),
        ("text".to_owned(), Some("文件".as_bytes().to_vec())),
        ("empty".to_owned(), Some(Vec::new())),
        ("flag".to_owned(), None),
    ]));
    let created = TestClient::post("http://localhost/uploads")
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("upload-length", "3", true)
        .add_header(
            "upload-metadata",
            "binary /wA=,nul AA==,text 5paH5Lu2,empty ,flag",
            true,
        )
        .send(&service)
        .await;
    assert_eq!(created.status_code, Some(StatusCode::CREATED));
    let url = format!(
        "http://localhost{}",
        created.headers()["location"].to_str().unwrap()
    );
    let head = TestClient::head(&url)
        .add_header("tus-resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert_eq!(head.status_code, Some(StatusCode::OK));
    let parsed =
        Metadata::parse_metadata(head.headers()["upload-metadata"].to_str().unwrap()).unwrap();
    assert_eq!(parsed.0, expected.0);
    drop(service);

    let reopened = DiskStore::new().disk_root(directory.path());
    let info = reopened
        .get_upload_file_info("binary-metadata")
        .await
        .unwrap();
    assert_eq!(info.metadata.unwrap().0, expected.0);
    let stored: serde_json::Value = serde_json::from_slice(
        &tokio::fs::read(directory.path().join("metadata/binary-metadata.json"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(stored["metadata"]["binary"], json!([255, 0]));
    assert_eq!(stored["metadata"]["empty"], json!([]));
    assert_eq!(stored["metadata"]["flag"], json!(null));

    let reopened_service = Service::new(Tus::new().path("/uploads").store(reopened).into_router());
    let head = TestClient::head(&url)
        .add_header("tus-resumable", "1.0.0", true)
        .send(&reopened_service)
        .await;
    assert_eq!(head.status_code, Some(StatusCode::OK));
    let parsed =
        Metadata::parse_metadata(head.headers()["upload-metadata"].to_str().unwrap()).unwrap();
    assert_eq!(parsed.0, expected.0);
}

#[tokio::test]
async fn legacy_text_records_are_read_and_rewritten_as_bytes() {
    let directory = tempfile::tempdir().unwrap();
    tokio::fs::create_dir(directory.path().join("data"))
        .await
        .unwrap();
    tokio::fs::create_dir(directory.path().join("metadata"))
        .await
        .unwrap();
    tokio::fs::write(directory.path().join("data/legacy.bin"), b"")
        .await
        .unwrap();
    let metadata_path = directory.path().join("metadata/legacy.json");
    let legacy = json!({
        "id": "legacy", "size": 3, "offset": 0,
        "metadata": {"text": "文件", "empty": "", "flag": null},
        "storage": null, "creation_date": "2026-10-29T00:00:00Z",
    });
    tokio::fs::write(&metadata_path, serde_json::to_vec(&legacy).unwrap())
        .await
        .unwrap();

    let store = DiskStore::new().disk_root(directory.path());
    let info = store.get_upload_file_info("legacy").await.unwrap();
    let metadata = info.metadata.unwrap();
    assert_eq!(metadata.get_text("text").unwrap(), Some("文件"));
    assert_eq!(metadata.get("empty"), Some(&Some(Vec::new())));
    assert_eq!(metadata.get("flag"), Some(&None));
    store.declare_upload_length("legacy", 3).await.unwrap();
    drop(store);

    let rewritten: serde_json::Value =
        serde_json::from_slice(&tokio::fs::read(&metadata_path).await.unwrap()).unwrap();
    assert_eq!(rewritten["metadata"]["text"], json!("文件".as_bytes()));
    assert_eq!(rewritten["metadata"]["empty"], json!([]));
    assert_eq!(rewritten["metadata"]["flag"], json!(null));
    let reopened = DiskStore::new().disk_root(directory.path());
    assert_eq!(
        reopened
            .get_upload_file_info("legacy")
            .await
            .unwrap()
            .metadata
            .unwrap()
            .0,
        metadata.0
    );
}

#[tokio::test]
async fn binary_filename_and_filetype_do_not_become_path_hints() {
    let directory = tempfile::tempdir().unwrap();
    let service = Service::new(
        Tus::new()
            .path("/uploads")
            .storage_root(directory.path())
            .upload_id_naming_function(|_, _| async { Ok("binary-hints".to_owned()) })
            .into_router(),
    );
    let created = TestClient::post("http://localhost/uploads")
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("upload-length", "3", true)
        .add_header("upload-metadata", "filename /w==,filetype /w==", true)
        .send(&service)
        .await;
    assert_eq!(created.status_code, Some(StatusCode::CREATED));
    let url = format!(
        "http://localhost{}",
        created.headers()["location"].to_str().unwrap()
    );
    let patched = TestClient::patch(&url)
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("content-type", "application/offset+octet-stream", true)
        .add_header("upload-offset", "0", true)
        .body("abc")
        .send(&service)
        .await;
    assert_eq!(patched.status_code, Some(StatusCode::NO_CONTENT));
    let reopened = DiskStore::new().disk_root(directory.path());
    let info = reopened.get_upload_file_info("binary-hints").await.unwrap();
    let metadata = info.metadata.unwrap();
    assert_eq!(metadata.get("filename"), Some(&Some(vec![0xff])));
    assert_eq!(metadata.get("filetype"), Some(&Some(vec![0xff])));
    let storage = info.storage.unwrap();
    assert_eq!(
        Path::new(&storage.path).file_name().unwrap(),
        "binary-hints.bin"
    );
    assert_eq!(tokio::fs::read(storage.path).await.unwrap(), b"abc");
}
