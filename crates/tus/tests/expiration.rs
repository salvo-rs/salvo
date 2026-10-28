//! Completed resources remain available after their upload deadline.

use std::collections::HashSet;
use std::time::Duration;

use salvo_core::http::{Method, StatusCode};
use salvo_core::test::TestClient;
use salvo_core::{Service, async_trait};
use salvo_tus::{ByteStream, DataStore, Extension, StoreInfo, Tus, TusResult, UploadInfo};

struct ExpiringStore {
    info: UploadInfo,
    expiration: Option<Duration>,
    enabled: bool,
}

#[async_trait]
impl DataStore for ExpiringStore {
    fn extensions(&self) -> HashSet<Extension> {
        if self.enabled {
            HashSet::from([Extension::Expiration])
        } else {
            HashSet::new()
        }
    }

    async fn create(&self, info: UploadInfo) -> TusResult<UploadInfo> {
        Ok(info)
    }

    async fn remove(&self, _: &str) -> TusResult<()> {
        Ok(())
    }

    async fn write(&self, _: &str, _: u64, _: ByteStream) -> TusResult<u64> {
        Ok(0)
    }

    async fn get_upload_file_info(&self, _: &str) -> TusResult<UploadInfo> {
        Ok(self.info.clone())
    }

    async fn declare_upload_length(&self, _: &str, _: u64) -> TusResult<()> {
        Ok(())
    }

    fn get_expiration(&self) -> Option<Duration> {
        self.expiration
    }
}

#[tokio::test]
async fn completed_uploads_outlive_their_upload_expiration() {
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), b"abc").unwrap();
    for (size, offset, expired, enabled, expected) in [
        (Some(3), 3, true, true, StatusCode::OK),
        (Some(0), 0, true, true, StatusCode::OK),
        (Some(3), 0, true, true, StatusCode::GONE),
        (None, 0, true, true, StatusCode::GONE),
        (Some(3), 0, false, true, StatusCode::OK),
        (None, 0, false, true, StatusCode::OK),
        (Some(3), 0, true, false, StatusCode::OK),
    ] {
        let created = chrono::Utc::now() - chrono::Duration::seconds(if expired { 120 } else { 0 });
        let store = ExpiringStore {
            info: UploadInfo {
                id: "upload".into(),
                size,
                offset: Some(offset),
                metadata: None,
                storage: Some(StoreInfo {
                    type_name: "file".into(),
                    path: file.path().to_string_lossy().into_owned(),
                    bucket: None,
                }),
                creation_date: created.to_rfc3339(),
            },
            expiration: Some(Duration::from_secs(60)),
            enabled,
        };
        let service = Service::new(Tus::new().path("uploads").store(store).into_router());
        for method in [Method::HEAD, Method::GET, Method::PATCH] {
            let mut request = TestClient::get("http://localhost/uploads/upload")
                .add_header("Tus-Resumable", "1.0.0", true)
                .add_header("Content-Type", "application/offset+octet-stream", true)
                .add_header("Upload-Offset", offset.to_string(), true)
                .build();
            *request.method_mut() = method.clone();
            let response = service.handle(request).await;
            let status = if method == Method::PATCH && expected == StatusCode::OK {
                StatusCode::NO_CONTENT
            } else {
                expected
            };
            assert_eq!(
                response.status_code,
                Some(status),
                "method={method}, size={size:?}, offset={offset}, expired={expired}, enabled={enabled}"
            );
            if size == Some(offset) {
                assert!(!response.headers().contains_key("Upload-Expires"));
            } else if method != Method::GET && expected == StatusCode::OK && enabled {
                assert!(response.headers().contains_key("Upload-Expires"));
            }
        }
    }
}
