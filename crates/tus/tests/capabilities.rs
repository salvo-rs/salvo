use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use salvo_core::http::StatusCode;
use salvo_core::test::TestClient;
use salvo_core::{Service, async_trait};
use salvo_tus::{ByteStream, DataStore, Extension, Tus, TusResult, UploadInfo};

struct ConcatStore {
    extensions: HashSet<Extension>,
    creates: Arc<AtomicUsize>,
}

#[async_trait]
impl DataStore for ConcatStore {
    fn extensions(&self) -> HashSet<Extension> {
        self.extensions.clone()
    }

    async fn create(&self, info: UploadInfo) -> TusResult<UploadInfo> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        Ok(info)
    }

    async fn remove(&self, _: &str) -> TusResult<()> {
        Ok(())
    }

    async fn write(&self, _: &str, _: u64, _: ByteStream) -> TusResult<u64> {
        Ok(0)
    }

    async fn get_upload_file_info(&self, _: &str) -> TusResult<UploadInfo> {
        unreachable!("capability requests do not read stored uploads")
    }

    async fn declare_upload_length(&self, _: &str, _: u64) -> TusResult<()> {
        Ok(())
    }
}

#[tokio::test]
async fn concatenation_cannot_be_enabled_by_a_custom_store() {
    for creation in [false, true] {
        let creates = Arc::new(AtomicUsize::new(0));
        let mut extensions = HashSet::from([Extension::Concatenation]);
        if creation {
            extensions.insert(Extension::Creation);
        }
        let service = Service::new(
            Tus::new()
                .path("uploads")
                .store(ConcatStore {
                    extensions,
                    creates: creates.clone(),
                })
                .into_router(),
        );
        for path in ["uploads", "uploads/upload"] {
            let response = TestClient::options(format!("http://localhost/{path}"))
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::NO_CONTENT));
            let advertised = response.headers().get("Tus-Extension");
            if creation {
                assert_eq!(advertised.unwrap(), "creation");
            } else {
                assert!(advertised.is_none());
            }
        }
        for value in ["partial", "final;/uploads/a /uploads/b"] {
            for with_length in [false, true] {
                let mut request = TestClient::post("http://localhost/uploads")
                    .add_header("Tus-Resumable", "1.0.0", true)
                    .add_header("Upload-Concat", value, true);
                if with_length {
                    request = request.add_header("Upload-Length", "3", true);
                }
                let response = request.send(&service).await;
                assert_eq!(response.status_code, Some(StatusCode::NOT_IMPLEMENTED));
            }
        }
        assert_eq!(creates.load(Ordering::SeqCst), 0);
        if creation {
            let response = TestClient::post("http://localhost/uploads")
                .add_header("Tus-Resumable", "1.0.0", true)
                .add_header("Upload-Length", "0", true)
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::CREATED));
            assert_eq!(creates.load(Ordering::SeqCst), 1);
        }
    }
}
