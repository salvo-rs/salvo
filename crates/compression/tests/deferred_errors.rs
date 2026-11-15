//! Error representations are negotiated after the service's configured catcher.
#![cfg(feature = "gzip")]
#![allow(missing_docs, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use http_body_util::BodyExt;
use salvo_compression::Compression;
use salvo_core::catcher::Catcher;
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};

#[handler]
async fn failure(req: &mut Request, res: &mut Response) {
    let status = req.query::<u16>("status").unwrap_or(404);
    res.add_header("x-original-error", status.to_string(), true)
        .unwrap();
    res.add_header("allow", "GET, HEAD", true).unwrap();
    res.render(StatusError::from_code(StatusCode::from_u16(status).unwrap()).unwrap());
}

#[handler]
async fn custom_error(res: &mut Response) {
    let status = res.status_code.unwrap().as_u16();
    res.add_header("x-catcher-status", status.to_string(), true)
        .unwrap();
    res.add_header("set-cookie", "catcher=preserved", false)
        .unwrap();
    res.render(Text::Plain(format!("caught {status}")));
}

#[tokio::test]
async fn deferred_errors_keep_their_status_and_use_the_custom_catcher_before_encoding() {
    let service = Service::new(Router::new().hoop(Compression::new()).goal(failure))
        .catcher(Catcher::new(custom_error));
    for status in [404, 500] {
        let mut response = TestClient::get(format!("http://localhost/?status={status}"))
            .add_header("accept-encoding", "gzip, identity;q=0", true)
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap().as_u16(), status);
        assert_eq!(response.headers()["x-catcher-status"], status.to_string());
        assert_eq!(response.headers()["content-encoding"], "gzip");
        assert_eq!(response.headers()["set-cookie"], "catcher=preserved");
        assert_eq!(
            response.take_string().await.unwrap(),
            format!("caught {status}")
        );
    }
}

#[tokio::test]
async fn unsupported_final_error_encoding_is_an_empty_406_without_recursing_into_the_catcher() {
    let calls = Arc::new(AtomicUsize::new(0));
    struct CountedCatcher(Arc<AtomicUsize>);
    #[salvo_core::async_trait]
    impl Handler for CountedCatcher {
        async fn handle(
            &self,
            req: &mut Request,
            depot: &mut Depot,
            res: &mut Response,
            ctrl: &mut FlowCtrl,
        ) {
            self.0.fetch_add(1, Ordering::SeqCst);
            custom_error.handle(req, depot, res, ctrl).await;
        }
    }
    let service = Service::new(Router::new().hoop(Compression::new()).goal(failure))
        .catcher(Catcher::new(CountedCatcher(calls.clone())));
    let mut response = TestClient::get("http://localhost/?status=500")
        .add_header("accept-encoding", "gzip;q=0, *;q=0, identity;q=0", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::NOT_ACCEPTABLE));
    assert_eq!(response.headers()["x-catcher-status"], "500");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(!response.headers().contains_key("content-encoding"));
    assert!(!response.headers().contains_key("content-type"));
    assert!(
        response
            .take_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty()
    );
}

#[tokio::test]
async fn default_catcher_preserves_the_original_deferred_error() {
    let mut response = TestClient::get("http://localhost/?status=404")
        .add_header("accept-encoding", "gzip, identity;q=0", true)
        .send(Router::new().hoop(Compression::new()).goal(failure))
        .await;
    assert_eq!(response.status_code, Some(StatusCode::NOT_FOUND));
    assert_eq!(response.headers()["content-encoding"], "gzip");
    assert!(response.take_string().await.unwrap().contains("404"));
}

#[tokio::test]
async fn head_deferred_error_keeps_error_headers_when_identity_is_rejected() {
    let service = Service::new(Router::new().hoop(Compression::new()).goal(failure))
        .catcher(Catcher::new(custom_error));
    for status in [404, 500] {
        let response = TestClient::head(format!("http://localhost/?status={status}"))
            .add_header("accept-encoding", "gzip, identity;q=0", true)
            .send(&service)
            .await;
        assert_eq!(response.status_code.unwrap().as_u16(), status);
        assert_eq!(response.headers()["x-original-error"], status.to_string());
        assert_eq!(response.headers()["allow"], "GET, HEAD");
        assert!(response.body.is_none());
    }
}

#[handler]
async fn success(res: &mut Response) {
    res.render("hello");
}

#[handler]
async fn outer(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
    ctrl.call_next(req, depot, res).await;
    assert_eq!(res.headers()["content-encoding"], "gzip");
    res.add_header("x-outer-saw-encoding", "gzip", true)
        .unwrap();
}

#[tokio::test]
async fn normal_success_compression_keeps_existing_middleware_order() {
    let mut response = TestClient::get("http://localhost/")
        .add_header("accept-encoding", "gzip, identity;q=0", true)
        .send(
            Router::new()
                .hoop(outer)
                .hoop(Compression::new())
                .goal(success),
        )
        .await;
    assert_eq!(response.headers()["x-outer-saw-encoding"], "gzip");
    assert_eq!(response.take_string().await.unwrap(), "hello");
}

#[tokio::test]
async fn bodyless_statuses_do_not_register_a_finalization_hook() {
    for status in [100, 101, 103, 204, 205, 304] {
        let mut req = TestClient::get("http://localhost/")
            .add_header("accept-encoding", "identity;q=0, *;q=0", true)
            .build();
        let mut response = Response::new();
        response.status_code(StatusCode::from_u16(status).unwrap());
        Compression::new()
            .handle(
                &mut req,
                &mut Depot::new(),
                &mut response,
                &mut FlowCtrl::new(Vec::new()),
            )
            .await;
        assert_eq!(response.status_code.unwrap().as_u16(), status);
        assert!(response.extensions.is_empty());
        assert!(response.body.is_none());
    }
}
