//! Downloads expose the committed upload snapshot, including after cancellation.

use std::time::Duration;

use bytes::Bytes;
use futures_util::{StreamExt, stream};
use salvo_core::Service;
use salvo_core::http::StatusCode;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_tus::{DataStore, DiskStore, Tus, UploadInfo};

async fn seed(store: &DiskStore, prefix: Bytes, size: u64) {
    store
        .create(UploadInfo {
            id: "snapshot".into(),
            size: Some(size),
            offset: Some(0),
            metadata: Some(salvo_tus::Metadata(std::collections::HashMap::from([(
                "filename".into(),
                Some(b"snapshot.dat".to_vec()),
            )]))),
            storage: None,
            creation_date: "2026-11-14T00:00:00Z".into(),
        })
        .await
        .unwrap();
    store
        .write("snapshot", 0, Box::pin(stream::iter([Ok(prefix)])))
        .await
        .unwrap();
}

#[tokio::test]
async fn cancelled_write_download_uses_committed_length_and_ranges() {
    let directory = tempfile::tempdir().unwrap();
    let store = DiskStore::new().disk_root(directory.path());
    seed(&store, Bytes::from_static(b"prefix"), 100).await;
    let writing_store = store.clone();
    let writing = tokio::spawn(async move {
        writing_store
            .write(
                "snapshot",
                6,
                Box::pin(
                    stream::iter([Ok(Bytes::from_static(b" UNCOMMITTED"))])
                        .chain(stream::pending()),
                ),
            )
            .await
    });
    // Wait for a real disk write, then cancel the pending body rather than
    // simulating an interrupted upload by editing its payload or metadata.
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if tokio::fs::metadata(directory.path().join("data/snapshot.bin"))
                .await
                .unwrap()
                .len()
                == 18
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    writing.abort();
    assert!(writing.await.unwrap_err().is_cancelled());
    let service = Service::new(
        Tus::new()
            .path("/uploads")
            .store(store.clone())
            .into_router(),
    );
    let url = "http://localhost/uploads/snapshot";
    let head = TestClient::head(url)
        .add_header("tus-resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert_eq!(head.headers()["upload-offset"], "6");
    let mut full = TestClient::get(url)
        .add_header("tus-resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert_eq!(full.status_code, Some(StatusCode::OK));
    assert_eq!(full.headers()["content-length"], "6");
    let etag = full.headers()["etag"].to_str().unwrap().to_owned();
    assert_eq!(full.take_bytes(None).await.unwrap(), "prefix");
    let mut partial = TestClient::get(url)
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("range", "bytes=4-20", true)
        .send(&service)
        .await;
    assert_eq!(partial.status_code, Some(StatusCode::PARTIAL_CONTENT));
    assert_eq!(partial.headers()["content-range"], "bytes 4-5/6");
    assert_eq!(partial.headers()["content-length"], "2");
    assert_eq!(partial.headers()["etag"], etag);
    assert_eq!(partial.take_bytes(None).await.unwrap(), "ix");
    let unsatisfiable = TestClient::get(url)
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("range", "bytes=6-", true)
        .send(&service)
        .await;
    assert_eq!(
        unsatisfiable.status_code,
        Some(StatusCode::RANGE_NOT_SATISFIABLE)
    );
    assert_eq!(unsatisfiable.headers()["content-range"], "bytes */6");
    let unchanged = TestClient::get(url)
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("if-none-match", &etag, true)
        .send(&service)
        .await;
    assert_eq!(unchanged.status_code, Some(StatusCode::NOT_MODIFIED));
    store
        .write(
            "snapshot",
            6,
            Box::pin(stream::iter([Ok(Bytes::from_static(b"suffix"))])),
        )
        .await
        .unwrap();
    let mut newer = TestClient::get(url)
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("if-range", &etag, true)
        .add_header("range", "bytes=0-1", true)
        .send(&service)
        .await;
    assert_eq!(newer.status_code, Some(StatusCode::OK));
    assert_ne!(newer.headers()["etag"], etag);
    assert_eq!(newer.headers()["content-length"], "12");
    assert_eq!(newer.take_bytes(None).await.unwrap(), "prefixsuffix");
}

#[tokio::test]
async fn streaming_download_blocks_completion_until_eof() {
    let directory = tempfile::tempdir().unwrap();
    let store = DiskStore::new().disk_root(directory.path());
    let prefix = Bytes::from(vec![b'p'; 2 * 1024 * 1024]);
    let offset = prefix.len();
    seed(&store, prefix.clone(), (offset + 6) as u64).await;
    let service = Service::new(Tus::new().path("/uploads").store(store).into_router());
    let url = "http://localhost/uploads/snapshot";
    let mut download = TestClient::get(url)
        .add_header("tus-resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert!(download.body.is_stream());
    let completion = TestClient::patch(url)
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("content-type", "application/offset+octet-stream", true)
        .add_header("upload-offset", offset.to_string(), true)
        .body("suffix")
        .send(&service);
    tokio::pin!(completion);
    std::future::poll_fn(|cx| {
        assert!(completion.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    assert_eq!(download.take_bytes(None).await.unwrap(), prefix);
    let completed = tokio::time::timeout(Duration::from_secs(3), completion)
        .await
        .unwrap();
    assert_eq!(completed.status_code, Some(StatusCode::NO_CONTENT));
    assert!(directory.path().join("data/snapshot.dat").exists());
    assert!(!directory.path().join("data/snapshot.bin").exists());
    let mut newer = TestClient::get(url)
        .add_header("tus-resumable", "1.0.0", true)
        .send(&service)
        .await;
    let bytes = newer.take_bytes(None).await.unwrap();
    assert_eq!(&bytes[..offset], &prefix[..]);
    assert_eq!(&bytes[offset..], b"suffix");
}

#[tokio::test]
async fn dropping_download_releases_delete_lock() {
    let directory = tempfile::tempdir().unwrap();
    let store = DiskStore::new().disk_root(directory.path());
    seed(
        &store,
        Bytes::from(vec![b'p'; 2 * 1024 * 1024]),
        4 * 1024 * 1024,
    )
    .await;
    let service = Service::new(Tus::new().path("/uploads").store(store).into_router());
    let url = "http://localhost/uploads/snapshot";
    let download = TestClient::get(url)
        .add_header("tus-resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert!(download.body.is_stream());
    let deletion = TestClient::delete(url)
        .add_header("tus-resumable", "1.0.0", true)
        .send(&service);
    tokio::pin!(deletion);
    std::future::poll_fn(|cx| {
        assert!(deletion.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    drop(download);
    let deleted = tokio::time::timeout(Duration::from_secs(3), deletion)
        .await
        .unwrap();
    assert_eq!(deleted.status_code, Some(StatusCode::NO_CONTENT));
    assert!(!directory.path().join("data/snapshot.bin").exists());
}
