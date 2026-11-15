//! Router configuration, canonical paths, trusted origins and lock deadlines.
use salvo_core::prelude::*;
use salvo_core::test::TestClient;
use salvo_tus::options::TusOptions;
use salvo_tus::{CancellationContext, Tus, TusError};

#[handler]
async fn vary(res: &mut Response) {
    res.add_header("vary", "accept-language", true).unwrap();
}

#[tokio::test]
async fn all_configuration_reaches_the_mounted_handlers() {
    let directory = tempfile::tempdir().unwrap();
    let options = TusOptions {
        path: "api/uploads/".into(),
        relative_location: false,
        disable_termination_for_finished_uploads: true,
        allowed_origins: vec!["https://client.example".into()],
        ..Default::default()
    };
    let mut tus = Tus::with_options(options)
        .storage_root(directory.path())
        .upload_id_naming_function(|_, _| async { Ok("id".into()) });
    assert_eq!(tus.options().path, "/api/uploads");
    tus.options_mut().path = "api/uploads/".into();
    let service = Service::new(Router::new().hoop(vary).push(tus.into_router()));
    let response = TestClient::post("https://uploads.example/api/uploads")
        .add_header("host", "uploads.example", true)
        .add_header("origin", "https://client.example", true)
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("upload-length", "0", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::CREATED));
    assert_eq!(
        response.headers()["location"],
        "https://uploads.example/api/uploads/id"
    );
    let varies = response
        .headers()
        .get_all("vary")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect::<Vec<_>>()
        .join(",")
        .to_ascii_lowercase();
    assert!(varies.contains("origin") && varies.contains("accept-language"));
    let response = TestClient::delete("https://uploads.example/api/uploads/id")
        .add_header("tus-resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::FORBIDDEN));
}

#[test]
fn authority_and_connection_scheme_define_direct_locations() {
    let options = TusOptions {
        relative_location: false,
        ..Default::default()
    };
    let mut request = TestClient::post("https://uploads.example/tus-files").build();
    request.headers_mut().remove("host");
    assert_eq!(
        options.generate_upload_url(&mut request, "id").unwrap(),
        "https://uploads.example/tus-files/id"
    );
    *request.version_mut() = salvo_core::http::Version::HTTP_2;
    request.add_header("host", "other.example", true).unwrap();
    assert_eq!(
        options.generate_upload_url(&mut request, "id").unwrap(),
        "https://uploads.example/tus-files/id"
    );
}

#[test]
fn absolute_form_authority_overrides_host_and_keeps_the_connection_scheme() {
    let options = TusOptions {
        relative_location: false,
        ..Default::default()
    };
    let mut request = TestClient::post("https://target.example/tus-files")
        .add_header("host", "conflicting.example", true)
        .build();
    *request.version_mut() = salvo_core::http::Version::HTTP_11;
    *request.scheme_mut() = salvo_core::http::uri::Scheme::HTTP;
    assert_eq!(
        options.generate_upload_url(&mut request, "id").unwrap(),
        "http://target.example/tus-files/id"
    );
}

#[test]
fn forwarded_values_only_override_when_enabled_and_keep_their_precedence() {
    let mut request = TestClient::post("https://direct.example/")
        .add_header("forwarded", "host=localhost;proto=http", true)
        .add_header("x-forwarded-host", "ignored.example", true)
        .add_header("x-forwarded-proto", "https", true)
        .build();
    let options = TusOptions {
        relative_location: false,
        respect_forwarded_headers: true,
        ..Default::default()
    };
    assert_eq!(
        options.generate_upload_url(&mut request, "id").unwrap(),
        "http://localhost/tus-files/id"
    );
}

#[tokio::test]
async fn configured_lock_deadline_returns_a_retryable_error() {
    let options = TusOptions {
        lock_drain_timeout: Some(1),
        ..Default::default()
    };
    let held = options.locker.write_lock("id").await.unwrap();
    let result = options
        .acquire_write_lock(&Request::new(), "id", CancellationContext::new())
        .await;
    assert!(matches!(result, Err(TusError::LockTimeout)));
    assert_eq!(
        result.err().unwrap().status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    drop(held);
    assert!(
        options
            .acquire_write_lock(&Request::new(), "id", CancellationContext::new())
            .await
            .is_ok()
    );
}
