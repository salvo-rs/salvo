//! Regression tests for cached request data across the Tower adapter.
#![cfg(feature = "tower-compat")]

use salvo_core::http::{ReqBody, header};
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_extra::tower_compat::TowerLayerCompat;
use tower::Service;

#[handler]
async fn cache_request(req: &mut Request) {
    assert_eq!(req.query::<String>("name").as_deref(), Some("old"));
    assert_eq!(req.payload().await.unwrap().as_ref(), b"name=old");
    assert_eq!(req.form::<String>("name").await.as_deref(), Some("old"));
}

#[handler]
async fn show_request(req: &mut Request) -> String {
    let query = req.query::<String>("name").unwrap();
    let body = String::from_utf8(req.payload().await.unwrap().to_vec()).unwrap();
    let form = req.form::<String>("name").await.unwrap();
    format!("{query}|{body}|{form}")
}

#[tokio::test]
async fn tower_rewrites_refresh_query_payload_and_form_caches() {
    let layer = tower::layer::layer_fn(|mut inner: salvo_extra::tower_compat::FlowCtrlService| {
        tower::service_fn(move |mut request: hyper::Request<ReqBody>| {
            *request.uri_mut() = "/?name=new".parse().unwrap();
            *request.body_mut() = ReqBody::from("name=new");
            inner.call(request)
        })
    });
    let router = Router::new()
        .hoop(cache_request)
        .hoop(layer.compat())
        .post(show_request);
    let mut response = TestClient::post("http://localhost/?name=old")
        .raw_form("name=old")
        .send(router)
        .await;
    assert_eq!(response.take_string().await.unwrap(), "new|name=new|new");
}

#[tokio::test]
async fn tower_passthrough_preserves_consumed_payload_and_form_caches() {
    let layer = tower::layer::layer_fn(|inner: salvo_extra::tower_compat::FlowCtrlService| inner);
    let router = Router::new()
        .hoop(cache_request)
        .hoop(layer.compat())
        .post(show_request);
    let mut response = TestClient::post("http://localhost/?name=old")
        .raw_form("name=old")
        .send(router)
        .await;
    assert_eq!(response.take_string().await.unwrap(), "old|name=old|old");
}

#[tokio::test]
async fn merge_hyper_refreshes_form_when_content_type_changes() {
    let mut req = Request::new();
    req.add_header(
        header::CONTENT_TYPE,
        "application/x-www-form-urlencoded",
        true,
    )
    .unwrap();
    req.replace_body(ReqBody::from("name=old"));
    assert_eq!(req.form::<String>("name").await.as_deref(), Some("old"));
    let mut hyper = req.strip_to_hyper::<ReqBody>().unwrap();
    hyper
        .headers_mut()
        .insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    req.merge_hyper(hyper);
    assert!(
        req.form_data().await.is_err(),
        "old form cannot survive a non-form Content-Type"
    );
}

#[tokio::test]
async fn merge_hyper_keeps_payload_when_only_headers_change() {
    let mut req = Request::new();
    req.replace_body(ReqBody::from("original"));
    assert_eq!(req.payload().await.unwrap().as_ref(), b"original");
    let mut hyper = req.strip_to_hyper::<ReqBody>().unwrap();
    hyper.headers_mut().insert("x-new", "new".parse().unwrap());
    req.merge_hyper(hyper);
    assert_eq!(req.payload().await.unwrap().as_ref(), b"original");
}

#[tokio::test]
async fn tower_passthrough_preserves_form_only_cache() {
    #[handler]
    async fn form_only(req: &mut Request) {
        assert_eq!(req.form::<String>("name").await.as_deref(), Some("old"));
    }
    #[handler]
    async fn show_form(req: &mut Request) -> String {
        req.form::<String>("name").await.unwrap()
    }
    let layer = tower::layer::layer_fn(|inner: salvo_extra::tower_compat::FlowCtrlService| inner);
    let router = Router::new()
        .hoop(form_only)
        .hoop(layer.compat())
        .post(show_form);
    let mut response = TestClient::post("http://localhost/")
        .raw_form("name=old")
        .send(router)
        .await;
    assert_eq!(response.take_string().await.unwrap(), "old");
}

#[tokio::test]
async fn merge_hyper_explicit_empty_body_clears_consumed_payload() {
    let mut req = Request::new();
    req.replace_body(ReqBody::from("old"));
    assert_eq!(req.payload().await.unwrap().as_ref(), b"old");
    let mut hyper = req.strip_to_hyper::<ReqBody>().unwrap();
    *hyper.body_mut() = ReqBody::from("");
    req.merge_hyper(hyper);
    assert!(req.payload().await.unwrap().is_empty());
}

async fn form_only_through_header_rewrite(initial: &str, rewritten: &str, body: &str) -> String {
    #[handler]
    async fn cache_form(req: &mut Request) {
        assert_eq!(req.form::<String>("name").await.as_deref(), Some("old"));
        assert!(
            req.body().is_none(),
            "form parsing must consume the uncached body"
        );
    }
    #[handler]
    async fn show_form(req: &mut Request) -> String {
        req.form::<String>("name").await.unwrap_or_default()
    }
    let rewritten = header::HeaderValue::from_str(rewritten).unwrap();
    let layer = tower::layer::layer_fn(
        move |mut inner: salvo_extra::tower_compat::FlowCtrlService| {
            let rewritten = rewritten.clone();
            tower::service_fn(move |mut request: hyper::Request<ReqBody>| {
                request
                    .headers_mut()
                    .insert(header::CONTENT_TYPE, rewritten.clone());
                inner.call(request)
            })
        },
    );
    let router = Router::new()
        .hoop(cache_form)
        .hoop(layer.compat())
        .post(show_form);
    TestClient::post("http://localhost/")
        .body(body.to_owned())
        .add_header(header::CONTENT_TYPE, initial, true)
        .send(router)
        .await
        .take_string()
        .await
        .unwrap()
}

#[tokio::test]
async fn tower_equivalent_form_content_types_preserve_consumed_form_cache() {
    let multipart = "--FormBoundary\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nold\r\n--FormBoundary--\r\n";
    for (initial, rewritten, body) in [
        (
            "application/x-www-form-urlencoded",
            "application/x-www-form-urlencoded; charset=utf-8",
            "name=old",
        ),
        (
            "application/x-www-form-urlencoded; charset=utf-8",
            "APPLICATION/X-WWW-FORM-URLENCODED; CHARSET=\"UTF-8\"",
            "name=old",
        ),
        (
            "application/x-www-form-urlencoded; charset=iso-8859-1",
            "application/x-www-form-urlencoded; charset=utf-8",
            "name=old",
        ),
        (
            "multipart/form-data; boundary=FormBoundary",
            "MULTIPART/FORM-DATA; boundary=\"FormBoundary\"; charset=utf-8",
            multipart,
        ),
        (
            "multipart/form-data; charset=utf-8; boundary=FormBoundary",
            "multipart/form-data; boundary=FormBoundary",
            multipart,
        ),
    ] {
        assert_eq!(
            form_only_through_header_rewrite(initial, rewritten, body).await,
            "old",
            "equivalent Content-Type rewrite: {initial} -> {rewritten}"
        );
    }
}

#[tokio::test]
async fn tower_different_form_content_types_discard_consumed_form_cache() {
    let multipart = "--FormBoundary\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nold\r\n--FormBoundary--\r\n";
    for (initial, rewritten, body) in [
        (
            "application/x-www-form-urlencoded",
            "application/json",
            "name=old",
        ),
        (
            "application/x-www-form-urlencoded",
            "multipart/form-data; boundary=FormBoundary",
            "name=old",
        ),
        (
            "multipart/form-data; boundary=FormBoundary",
            "multipart/form-data; boundary=formboundary",
            multipart,
        ),
        (
            "multipart/form-data; boundary=FormBoundary",
            "multipart/form-data",
            multipart,
        ),
        (
            "multipart/form-data; boundary=FormBoundary",
            "multipart/mixed; boundary=FormBoundary",
            multipart,
        ),
        (
            "multipart/form-data; boundary=FormBoundary",
            "application/x-www-form-urlencoded",
            multipart,
        ),
        (
            "multipart/form-data; boundary=FormBoundary",
            "invalid-content-type",
            multipart,
        ),
    ] {
        assert_eq!(
            form_only_through_header_rewrite(initial, rewritten, body).await,
            "",
            "different Content-Type rewrite: {initial} -> {rewritten}"
        );
    }
}
