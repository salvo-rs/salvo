//! Completion hooks must preserve all values of additional response fields.

use salvo_core::http::{HeaderMap, HeaderValue, StatusCode};
use salvo_core::test::TestClient;
use salvo_core::{Response, Router, Service, handler};
use salvo_tus::Tus;
use salvo_tus::options::UploadFinishPatch;

#[handler]
async fn existing_headers(res: &mut Response) {
    res.headers_mut()
        .append("x-existing", HeaderValue::from_static("outer-one"));
    res.headers_mut()
        .append("x-existing", HeaderValue::from_static("outer-two"));
}

fn service(root: &std::path::Path) -> Service {
    let tus = Tus::new()
        .path("uploads")
        .storage_root(root)
        .on_upload_finish(|_, _| async {
            let mut headers = HeaderMap::new();
            for value in ["session=one; Path=/", "csrf=two; Path=/"] {
                headers.append("set-cookie", HeaderValue::from_static(value));
            }
            for value in ["first", "second"] {
                headers.append("x-hook", HeaderValue::from_static(value));
                headers.append("x-existing", HeaderValue::from_static(value));
            }
            headers.insert("tus-resumable", HeaderValue::from_static("invalid"));
            headers.insert("cache-control", HeaderValue::from_static("public"));
            Ok(UploadFinishPatch {
                headers: Some(headers),
                ..Default::default()
            })
        });
    Service::new(Router::with_hoop(existing_headers).push(tus.into_router()))
}

fn check_headers(response: &Response) {
    let values = |name| {
        response
            .headers()
            .get_all(name)
            .iter()
            .map(|value| value.to_str().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        values("set-cookie"),
        ["session=one; Path=/", "csrf=two; Path=/"]
    );
    assert_eq!(values("x-hook"), ["first", "second"]);
    assert_eq!(values("x-existing"), ["outer-one", "outer-two"]);
    assert_eq!(response.headers()["tus-resumable"], "1.0.0");
    assert_eq!(response.headers()["cache-control"], "no-store");
}

#[tokio::test]
async fn creation_finish_preserves_repeated_hook_headers() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path());
    for (length, body) in [("0", ""), ("3", "abc")] {
        let response = TestClient::post("http://localhost/uploads")
            .add_header("tus-resumable", "1.0.0", true)
            .add_header("upload-length", length, true)
            .add_header("content-type", "application/offset+octet-stream", true)
            .body(body)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::CREATED));
        check_headers(&response);
    }
}

#[tokio::test]
async fn patch_finish_preserves_repeated_hook_headers() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path());
    let response = TestClient::post("http://localhost/uploads")
        .add_header("tus-resumable", "1.0.0", true)
        .add_header("upload-length", "3", true)
        .send(&service)
        .await;
    let location = response.headers()["location"].to_str().unwrap();
    let response = TestClient::patch(format!(
        "http://localhost/{}",
        location.trim_start_matches('/')
    ))
    .add_header("tus-resumable", "1.0.0", true)
    .add_header("upload-offset", "0", true)
    .add_header("content-type", "application/offset+octet-stream", true)
    .body("abc")
    .send(&service)
    .await;
    assert_eq!(response.status_code, Some(StatusCode::NO_CONTENT));
    assert_eq!(response.headers()["upload-offset"], "3");
    check_headers(&response);
}
