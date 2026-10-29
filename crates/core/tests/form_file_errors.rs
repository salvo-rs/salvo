//! File extractors must preserve multipart parsing errors and HTTP 413.

#![cfg(feature = "test")]

use salvo_core::extract::{Extractible, FormFile, FormFiles};
use salvo_core::http::{ParseError, StatusCode};
use salvo_core::test::TestClient;
use salvo_core::{Depot, Request, Router, Service, Writer, handler};

fn request() -> Request {
    TestClient::post("http://localhost/")
        .add_header("content-type", "multipart/form-data; boundary=b", true)
        .body("--b\r\nContent-Disposition: form-data; name=\"files\"; filename=\"a.txt\"\r\n\r\ncontent\r\n--b--\r\n")
        .build()
}

#[tokio::test]
async fn file_extractors_preserve_byte_and_count_limit_errors() {
    for multiple in [false, true] {
        for byte_limit in [false, true] {
            let mut req = request();
            if byte_limit {
                req.set_secure_max_size(8);
            } else {
                req.set_form_limits(salvo_core::http::form::FormLimits::new().max_files(0));
            }
            let error = if multiple {
                FormFiles::extract_with_arg(&mut req, &mut Depot::new(), "files")
                    .await
                    .unwrap_err()
            } else {
                FormFile::extract_with_arg(&mut req, &mut Depot::new(), "files")
                    .await
                    .unwrap_err()
            };
            assert!(matches!(error, ParseError::PayloadTooLarge), "{error:?}");
        }
    }
}

#[handler]
async fn limit(req: &mut Request) {
    req.set_secure_max_size(8);
}

#[handler]
async fn single_file(files: FormFile) -> String {
    format!("executed:{}", files.size())
}

#[handler]
async fn multiple_files(files: FormFiles) -> String {
    format!("executed:{}", files.len())
}

#[tokio::test]
async fn oversized_file_extractors_return_http_413() {
    let service = Service::new(
        Router::new()
            .hoop(limit)
            .push(Router::with_path("single").post(single_file))
            .push(Router::with_path("multiple").post(multiple_files)),
    );
    for path in ["single", "multiple"] {
        let response = TestClient::post(format!("http://localhost/{path}"))
            .add_header("content-type", "multipart/form-data; boundary=b", true)
            .body("--b\r\nContent-Disposition: form-data; name=\"files\"; filename=\"a.txt\"\r\n\r\ncontent\r\n--b--\r\n")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::PAYLOAD_TOO_LARGE));
    }
}

#[tokio::test]
async fn file_extractors_keep_success_and_missing_file_behavior() {
    let mut req = request();
    assert_eq!(
        FormFile::extract_with_arg(&mut req, &mut Depot::new(), "files")
            .await
            .unwrap()
            .size(),
        7
    );
    assert_eq!(
        FormFiles::extract_with_arg(&mut req, &mut Depot::new(), "files")
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        FormFile::extract_with_arg(&mut req, &mut Depot::new(), "missing")
            .await
            .is_err()
    );
    assert!(
        FormFiles::extract_with_arg(&mut req, &mut Depot::new(), "missing")
            .await
            .is_err()
    );
}
