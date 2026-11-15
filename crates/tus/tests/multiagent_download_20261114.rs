//! Review probe for the default disposition of uploaded active content.
use salvo_core::Service;
use salvo_core::http::StatusCode;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_tus::{DiskStore, Tus};

#[tokio::test]
async fn uploaded_html_is_returned_inline() {
    let directory = tempfile::tempdir().unwrap();
    let service = Service::new(
        Tus::new()
            .path("/uploads")
            .store(DiskStore::new().disk_root(directory.path()))
            .into_router(),
    );
    let payload = "<!doctype html><script>document.title='review-marker'</script>";
    let created = TestClient::post("http://localhost/uploads")
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("upload-length", payload.len().to_string(), true)
        .add_header("upload-metadata", "filename YXR0YWNrLmh0bWw=", true)
        .add_header("content-type", "application/offset+octet-stream", true)
        .body(payload)
        .send(&service)
        .await;
    assert_eq!(created.status_code, Some(StatusCode::CREATED));
    let location = created.headers()["location"].to_str().unwrap();
    let url = if location.starts_with('/') {
        format!("http://localhost{location}")
    } else {
        location.to_owned()
    };
    let mut response = TestClient::get(url)
        .add_header("tus-resumable", "1.0.0", true)
        .send(&service)
        .await;
    println!(
        "download_status={:?}; content_type={:?}; content_disposition={:?}; csp={:?}",
        response.status_code,
        response.headers().get("content-type"),
        response.headers().get("content-disposition"),
        response.headers().get("content-security-policy")
    );
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert!(
        response.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("inline")
    );
    assert_eq!(response.take_string().await.unwrap(), payload);
}
