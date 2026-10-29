//! Method overrides retain routing, authorization, and response semantics.

use salvo_core::http::{Method, StatusCode};
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::{Depot, FlowCtrl, Request, Response, Router, Service, handler};
use salvo_tus::{DiskStore, Tus};

#[handler]
async fn record_method(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
) {
    let method = req.method().clone();
    ctrl.call_next(req, depot, res).await;
    assert_eq!(req.method(), method);
    res.headers_mut()
        .insert("x-effective-method", method.as_str().parse().unwrap());
}

#[handler]
async fn deny_delete(req: &mut Request, res: &mut Response, ctrl: &mut FlowCtrl) {
    if req.method() == Method::DELETE {
        res.status_code(StatusCode::FORBIDDEN);
        ctrl.skip_rest();
    }
}

#[handler]
async fn echo_method(req: &mut Request) -> String {
    req.method().to_string()
}

#[tokio::test]
async fn overrides_select_endpoints_before_outer_hoops() {
    let root = tempfile::tempdir().unwrap();
    let tus = Tus::new()
        .path("uploads")
        .store(DiskStore::new().disk_root(root.path()));
    let service = Service::new(
        Router::with_hoop(record_method)
            .hoop(deny_delete)
            .push(tus.clone().into_router()),
    );
    let response = TestClient::post("http://localhost/uploads")
        .add_header("Tus-Resumable", "1.0.0", true)
        .add_header("Upload-Length", "4", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::CREATED));
    let location = response
        .headers()
        .get("Location")
        .unwrap()
        .to_str()
        .unwrap();
    let url = format!("http://localhost/{}", location.trim_start_matches('/'));

    let response = TestClient::post(&url)
        .add_header("X-HTTP-Method-Override", "PATCH", true)
        .add_header("Tus-Resumable", "1.0.0", true)
        .add_header("Content-Type", "application/offset+octet-stream", true)
        .add_header("Upload-Offset", "0", true)
        .body("abc")
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::NO_CONTENT));
    assert_eq!(response.headers().get("Upload-Offset").unwrap(), "3");
    assert_eq!(
        response.headers().get("X-Effective-Method").unwrap(),
        "PATCH"
    );

    let mut response = TestClient::post(&url)
        .add_header("X-HTTP-Method-Override", "HEAD", true)
        .add_header("Tus-Resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(response.headers().get("Upload-Offset").unwrap(), "3");
    assert_eq!(
        response.headers().get("X-Effective-Method").unwrap(),
        "HEAD"
    );
    assert!(response.take_bytes(None).await.unwrap().is_empty());

    let response = TestClient::post(&url)
        .add_header("X-HTTP-Method-Override", "DELETE", true)
        .add_header("Tus-Resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::FORBIDDEN));
    assert_eq!(
        response.headers().get("X-Effective-Method").unwrap(),
        "DELETE"
    );
    let response = TestClient::head(&url)
        .add_header("Tus-Resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));

    let response = TestClient::post(&url)
        .add_header("X-HTTP-Method-Override", "DELETE", true)
        .add_header("Tus-Resumable", "1.0.0", true)
        .send(tus.into_router())
        .await;
    assert_eq!(response.status_code, Some(StatusCode::NO_CONTENT));
}

#[tokio::test]
async fn malformed_unknown_and_non_tus_overrides_are_isolated() {
    let service = Service::new(
        Router::new()
            .push(Tus::new().path("uploads").into_router())
            .push(Router::with_path("other").post(echo_method))
            // A prefix match without a complete tus endpoint must not mutate
            // the method subsequently seen by this sibling application route.
            .push(Router::with_path("uploads/{id}/other").post(echo_method)),
    );
    for value in ["", "PATCH DELETE", "PATCH,DELETE"] {
        let response = TestClient::post("http://localhost/uploads/upload")
            .add_header("X-HTTP-Method-Override", value, true)
            .add_header("Tus-Resumable", "1.0.0", true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::BAD_REQUEST));
    }
    let response = TestClient::post("http://localhost/uploads/upload")
        .add_header("X-HTTP-Method-Override", "PATCH", true)
        .add_header("X-HTTP-Method-Override", "DELETE", false)
        .add_header("Tus-Resumable", "1.0.0", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::BAD_REQUEST));
    let response = TestClient::post("http://localhost/uploads/upload")
        .add_header("X-HTTP-Method-Override", "BREW", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::METHOD_NOT_ALLOWED));
    let response = TestClient::post("http://localhost/uploads/upload")
        .add_header("X-HTTP-Method-Override", "PATCH", true)
        .add_header("Tus-Resumable", "0.9.0", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::PRECONDITION_FAILED));
    for path in ["other", "uploads/upload/other"] {
        let mut response = TestClient::post(format!("http://localhost/{path}"))
            .add_header("X-HTTP-Method-Override", "DELETE", true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.take_string().await.unwrap(), "POST");
    }
    let response = TestClient::post("http://localhost/uploads")
        .add_header("X-HTTP-Method-Override", "OPTIONS", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::NO_CONTENT));
    assert!(
        response
            .headers()
            .get("Access-Control-Allow-Headers")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("X-HTTP-Method-Override")
    );
}
