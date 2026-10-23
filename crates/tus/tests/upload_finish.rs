//! Upload completion behavior through the public tus HTTP endpoints.

use std::sync::{Arc, Mutex};

use salvo_core::Service;
use salvo_core::http::StatusCode;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_tus::options::UploadFinishPatch;
use salvo_tus::{Tus, TusError};

fn streamed_body(
    frames: Vec<Result<salvo_core::hyper::body::Frame<bytes::Bytes>, salvo_core::BoxedError>>,
) -> salvo_core::http::ReqBody {
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use bytes::Bytes;
    use salvo_core::hyper::body::{Body, Frame};

    struct Frames(VecDeque<Result<Frame<Bytes>, salvo_core::BoxedError>>);

    impl Body for Frames {
        type Data = Bytes;
        type Error = salvo_core::BoxedError;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
            Poll::Ready(self.0.pop_front())
        }
    }

    salvo_core::http::ReqBody::Boxed {
        inner: Box::pin(Frames(frames.into())),
        fuse_config: None,
    }
}

#[tokio::test]
async fn deferred_completion_waits_for_a_valid_request_stream() {
    use bytes::Bytes;
    use salvo_core::hyper::body::Frame;

    for initial in ["", "abc"] {
        for interrupted in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let completed = Arc::new(Mutex::new(Vec::new()));
            let observed = completed.clone();
            let service = Service::new(
                Tus::new()
                    .path("/uploads")
                    .storage_root(directory.path())
                    .on_upload_finish(move |_req, upload| {
                        observed.lock().unwrap().push(upload);
                        async { Ok(UploadFinishPatch::default()) }
                    })
                    .into_router(),
            );
            let created = TestClient::post("http://localhost/uploads")
                .add_header("tus-resumable", "1.0.0", true)
                .add_header("upload-defer-length", "1", true)
                .send(&service)
                .await;
            assert_eq!(created.status_code, Some(StatusCode::CREATED));
            let url = format!(
                "http://localhost{}",
                created.headers()["location"].to_str().unwrap()
            );
            let response = TestClient::patch(&url)
                .add_header("tus-resumable", "1.0.0", true)
                .add_header("content-type", "application/offset+octet-stream", true)
                .add_header("upload-offset", "0", true)
                .body(initial)
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::NO_CONTENT));

            let last_frame = if interrupted {
                Err(
                    std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "upload interrupted")
                        .into(),
                )
            } else {
                Ok(Frame::data(Bytes::from_static(b"x")))
            };
            // A stream with unknown length must be checked past empty frames,
            // before its length declaration can mark the upload as completed.
            let response = TestClient::patch(&url)
                .add_header("tus-resumable", "1.0.0", true)
                .add_header("content-type", "application/offset+octet-stream", true)
                .add_header("upload-offset", initial.len().to_string(), true)
                .add_header("upload-length", initial.len().to_string(), true)
                .body(streamed_body(vec![
                    Ok(Frame::data(Bytes::new())),
                    last_frame,
                ]))
                .send(&service)
                .await;
            assert_eq!(
                response.status_code,
                Some(if interrupted {
                    StatusCode::INTERNAL_SERVER_ERROR
                } else {
                    StatusCode::PAYLOAD_TOO_LARGE
                })
            );
            assert!(completed.lock().unwrap().is_empty());
            let head = TestClient::head(&url)
                .add_header("tus-resumable", "1.0.0", true)
                .send(&service)
                .await;
            assert_eq!(head.headers()["upload-offset"], initial.len().to_string());
            assert!(!head.headers().contains_key("upload-length"));
            assert_eq!(head.headers()["upload-defer-length"], "1");

            let response = TestClient::patch(&url)
                .add_header("tus-resumable", "1.0.0", true)
                .add_header("content-type", "application/offset+octet-stream", true)
                .add_header("upload-offset", initial.len().to_string(), true)
                .add_header("upload-length", initial.len().to_string(), true)
                .body(streamed_body(vec![
                    Ok(Frame::data(Bytes::new())),
                    Ok(Frame::trailers(salvo_core::http::HeaderMap::new())),
                ]))
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::NO_CONTENT));
            let response = TestClient::patch(&url)
                .add_header("tus-resumable", "1.0.0", true)
                .add_header("content-type", "application/offset+octet-stream", true)
                .add_header("upload-offset", initial.len().to_string(), true)
                .body("")
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::NO_CONTENT));
            let uploads = completed.lock().unwrap();
            assert_eq!(uploads.len(), 1);
            assert_eq!(uploads[0].offset, Some(initial.len() as u64));
            assert_eq!(uploads[0].size, uploads[0].offset);
            let stored = &uploads[0].storage.as_ref().unwrap().path;
            assert_eq!(std::fs::read(stored).unwrap(), initial.as_bytes());
        }
    }
}

#[tokio::test]
async fn patch_notifies_only_when_upload_reaches_completion() {
    let directory = tempfile::tempdir().unwrap();
    let completed = Arc::new(Mutex::new(Vec::new()));
    let observed = completed.clone();
    let service = Service::new(
        Tus::new()
            .path("/uploads")
            .storage_root(directory.path())
            .on_upload_finish(move |_req, upload| {
                observed.lock().unwrap().push(upload);
                async { Ok(UploadFinishPatch::default()) }
            })
            .into_router(),
    );
    let created = TestClient::post("http://localhost/uploads")
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("upload-length", "6", true)
        .send(&service)
        .await;
    assert_eq!(created.status_code, Some(StatusCode::CREATED));
    let location = created.headers()["location"].to_str().unwrap();
    let url = format!("http://localhost{location}");

    for (offset, body, expected_calls) in [(0, "abc", 0), (3, "def", 1), (6, "", 1)] {
        let response = TestClient::patch(&url)
            .add_header("tus-resumable", "1.0.0", true)
            .add_header("content-type", "application/offset+octet-stream", true)
            .add_header("upload-offset", offset.to_string(), true)
            .body(body)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NO_CONTENT));
        assert_eq!(completed.lock().unwrap().len(), expected_calls);
    }
    let uploads = completed.lock().unwrap();
    assert_eq!(uploads[0].offset, Some(6));
    assert_eq!(uploads[0].size, Some(6));
    assert!(uploads[0].storage.is_some());
}

#[tokio::test]
async fn patch_applies_finish_response_and_propagates_hook_errors() {
    for fail in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let service = Service::new(
            Tus::new()
                .path("/uploads")
                .storage_root(directory.path())
                .on_upload_finish(move |_req, _upload| async move {
                    if fail {
                        Err(TusError::Internal("completion rejected".into()))
                    } else {
                        let mut headers = salvo_core::http::HeaderMap::new();
                        headers.insert("x-upload-complete", "yes".parse().unwrap());
                        Ok(UploadFinishPatch {
                            status_code: Some(StatusCode::OK),
                            headers: Some(headers),
                            body: Some(b"finished".to_vec()),
                        })
                    }
                })
                .into_router(),
        );
        let created = TestClient::post("http://localhost/uploads")
            .add_header("tus-resumable", "1.0.0", true)
            .add_header("upload-length", "1", true)
            .send(&service)
            .await;
        assert_eq!(created.status_code, Some(StatusCode::CREATED));
        let location = created.headers()["location"].to_str().unwrap();
        let mut response = TestClient::patch(format!("http://localhost{location}"))
            .add_header("tus-resumable", "1.0.0", true)
            .add_header("content-type", "application/offset+octet-stream", true)
            .add_header("upload-offset", "0", true)
            .body("x")
            .send(&service)
            .await;
        assert_eq!(
            response.status_code,
            Some(if fail {
                StatusCode::INTERNAL_SERVER_ERROR
            } else {
                StatusCode::OK
            })
        );
        if !fail {
            assert_eq!(response.headers()["x-upload-complete"], "yes");
            assert_eq!(response.headers()["upload-offset"], "1");
            assert_eq!(response.take_string().await.unwrap(), "finished");
        }
    }
}

#[tokio::test]
async fn creation_and_deferred_length_uploads_notify_completion() {
    for mode in ["empty", "creation-with-upload", "deferred"] {
        let directory = tempfile::tempdir().unwrap();
        let completed = Arc::new(Mutex::new(Vec::new()));
        let observed = completed.clone();
        let service = Service::new(
            Tus::new()
                .path("/uploads")
                .storage_root(directory.path())
                .on_upload_finish(move |_req, upload| {
                    observed.lock().unwrap().push(upload);
                    async { Ok(UploadFinishPatch::default()) }
                })
                .into_router(),
        );
        let mut request = TestClient::post("http://localhost/uploads")
            .add_header("tus-resumable", "1.0.0", true)
            .add_header("upload-metadata", "filename ZmluaXNoZWQudHh0", true);
        request = match mode {
            "empty" => request.add_header("upload-length", "0", true),
            "creation-with-upload" => request
                .add_header("upload-length", "3", true)
                .add_header("content-type", "application/offset+octet-stream", true)
                .add_header("content-length", "3", true)
                .body("abc"),
            _ => request.add_header("upload-defer-length", "1", true),
        };
        let response = request.send(&service).await;
        assert_eq!(response.status_code, Some(StatusCode::CREATED), "{mode}");
        if mode == "deferred" {
            assert!(completed.lock().unwrap().is_empty());
            let location = response.headers()["location"].to_str().unwrap();
            let response = TestClient::patch(format!("http://localhost{location}"))
                .add_header("tus-resumable", "1.0.0", true)
                .add_header("content-type", "application/offset+octet-stream", true)
                .add_header("upload-offset", "0", true)
                .add_header("upload-length", "3", true)
                .body("abc")
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::NO_CONTENT));
        }
        let uploads = completed.lock().unwrap();
        assert_eq!(uploads.len(), 1, "{mode}");
        assert_eq!(uploads[0].size, Some(if mode == "empty" { 0 } else { 3 }));
        assert_eq!(uploads[0].offset, uploads[0].size);
        let stored = &uploads[0]
            .storage
            .as_ref()
            .expect("final storage info")
            .path;
        assert_eq!(
            std::path::Path::new(stored).file_name().unwrap(),
            "finished.txt"
        );
        assert_eq!(
            std::fs::read(stored).unwrap(),
            if mode == "empty" {
                b"".as_slice()
            } else {
                b"abc"
            }
        );
    }
}

#[tokio::test]
async fn rejected_creation_does_not_leave_upload_files() {
    use bytes::Bytes;
    use salvo_core::hyper::body::Frame;

    for mode in [
        "length",
        "invalid-length",
        "stream-limit",
        "interrupted",
        "invalid-url",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut tus = Tus::new().path("/uploads").storage_root(directory.path());
        if mode == "invalid-url" {
            tus = tus.generate_url_function(|_, _| Ok("/uploads/invalid\nlocation".into()));
        }
        let service = Service::new(tus.into_router());
        let mut request = TestClient::post("http://localhost/uploads")
            .add_header("tus-resumable", "1.0.0", true)
            .add_header("upload-length", "1", true)
            .add_header("content-type", "application/offset+octet-stream", true);
        request = match mode {
            "length" => request.add_header("content-length", "2", true).body("ab"),
            "invalid-length" => request.add_header("content-length", "invalid", true),
            "stream-limit" => request.body(streamed_body(vec![
                Ok(Frame::data(Bytes::from_static(b"a"))),
                Ok(Frame::data(Bytes::from_static(b"b"))),
            ])),
            "interrupted" => request.body(streamed_body(vec![
                Ok(Frame::data(Bytes::from_static(b"a"))),
                Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "interrupted").into()),
            ])),
            _ => request.body("a"),
        };
        let response = request.send(&service).await;
        let expected = match mode {
            "length" | "stream-limit" => StatusCode::PAYLOAD_TOO_LARGE,
            "invalid-length" => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        assert_eq!(response.status_code, Some(expected), "{mode}");
        assert!(!response.headers().contains_key("location"), "{mode}");
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            0,
            "{mode}"
        );
    }
}

#[tokio::test]
async fn creation_removes_upload_when_final_metadata_cannot_be_read() {
    use std::sync::atomic::{AtomicBool, Ordering};

    use salvo_tus::stores::{ByteStream, DataStore, UploadInfo};

    struct UnreadableStore(Arc<AtomicBool>);

    #[salvo_core::async_trait]
    impl DataStore for UnreadableStore {
        async fn create(&self, upload: UploadInfo) -> Result<UploadInfo, TusError> {
            Ok(upload)
        }
        async fn remove(&self, _id: &str) -> Result<(), TusError> {
            self.0.store(true, Ordering::SeqCst);
            Ok(())
        }
        async fn write(
            &self,
            _id: &str,
            _offset: u64,
            _stream: ByteStream,
        ) -> Result<u64, TusError> {
            Err(TusError::Internal("unexpected write".into()))
        }
        async fn get_upload_file_info(&self, _id: &str) -> Result<UploadInfo, TusError> {
            Err(TusError::Internal("metadata unavailable".into()))
        }
        async fn declare_upload_length(&self, _id: &str, _length: u64) -> Result<(), TusError> {
            Err(TusError::Internal("unexpected length declaration".into()))
        }
    }

    let removed = Arc::new(AtomicBool::new(false));
    let service = Service::new(
        Tus::new()
            .path("/uploads")
            .store(UnreadableStore(removed.clone()))
            .on_upload_finish(|_, _| async {
                panic!("completion must not run with missing metadata")
            })
            .into_router(),
    );
    let response = TestClient::post("http://localhost/uploads")
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("upload-length", "0", true)
        .send(&service)
        .await;
    assert_eq!(
        response.status_code,
        Some(StatusCode::INTERNAL_SERVER_ERROR)
    );
    assert!(!response.headers().contains_key("location"));
    assert!(removed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn completion_body_without_status_uses_ok() {
    let directory = tempfile::tempdir().unwrap();
    let service = Service::new(
        Tus::new()
            .path("/uploads")
            .storage_root(directory.path())
            .on_upload_finish(|_req, _upload| async {
                Ok(UploadFinishPatch {
                    body: Some(b"finished".to_vec()),
                    ..UploadFinishPatch::default()
                })
            })
            .into_router(),
    );
    let created = TestClient::post("http://localhost/uploads")
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("upload-length", "1", true)
        .send(&service)
        .await;
    let location = created.headers()["location"].to_str().unwrap();
    let mut response = TestClient::patch(format!("http://localhost{location}"))
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("content-type", "application/offset+octet-stream", true)
        .add_header("upload-offset", "0", true)
        .body("x")
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(response.take_string().await.unwrap(), "finished");
}

#[tokio::test]
async fn rejected_deferred_length_does_not_complete_the_stored_upload() {
    let directory = tempfile::tempdir().unwrap();
    let completed = Arc::new(Mutex::new(Vec::new()));
    let observed = completed.clone();
    let service = Service::new(
        Tus::new()
            .path("/uploads")
            .storage_root(directory.path())
            .on_upload_finish(move |_req, upload| {
                observed.lock().unwrap().push(upload);
                async { Ok(UploadFinishPatch::default()) }
            })
            .into_router(),
    );
    let created = TestClient::post("http://localhost/uploads")
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("upload-defer-length", "1", true)
        .send(&service)
        .await;
    let location = created.headers()["location"].to_str().unwrap();
    let url = format!("http://localhost{location}");
    let response = TestClient::patch(&url)
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("content-type", "application/offset+octet-stream", true)
        .add_header("upload-offset", "0", true)
        .body("abc")
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::NO_CONTENT));

    let response = TestClient::patch(&url)
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("content-type", "application/offset+octet-stream", true)
        .add_header("upload-offset", "3", true)
        .add_header("upload-length", "3", true)
        .add_header("content-length", "1", true)
        .body("x")
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::PAYLOAD_TOO_LARGE));
    assert!(completed.lock().unwrap().is_empty());
    let response = TestClient::head(&url)
        .add_header("tus-resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert_eq!(response.headers()["upload-offset"], "3");
    assert!(!response.headers().contains_key("upload-length"));
    assert_eq!(response.headers()["upload-defer-length"], "1");

    let response = TestClient::patch(&url)
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("content-type", "application/offset+octet-stream", true)
        .add_header("upload-offset", "3", true)
        .add_header("upload-length", "3", true)
        .body("")
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::NO_CONTENT));
    let uploads = completed.lock().unwrap();
    assert_eq!(uploads.len(), 1);
    assert_eq!(uploads[0].offset, Some(3));
    assert_eq!(uploads[0].size, Some(3));
}
