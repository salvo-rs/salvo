//! Conditional requests must reach the origin handler, even with a warm cache.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use salvo_cache::{Cache, MokaStore, RequestIssuer};
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use tokio::sync::Notify;

#[derive(Debug)]
struct ConditionalOrigin {
    calls: Arc<AtomicUsize>,
    started: Option<Arc<Notify>>,
    release: Option<Arc<Notify>>,
}

#[async_trait]
impl Handler for ConditionalOrigin {
    async fn handle(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if req.headers().contains_key("if-none-match") {
            res.status_code(StatusCode::NOT_MODIFIED);
            return;
        }
        if req.headers().contains_key("if-match") {
            res.status_code(StatusCode::PRECONDITION_FAILED);
            return;
        }
        for header in ["if-modified-since", "if-unmodified-since", "if-range"] {
            if req.headers().contains_key(header) {
                res.render(header);
                return;
            }
        }
        if let Some(started) = &self.started {
            started.notify_one();
        }
        if let Some(release) = &self.release {
            release.notified().await;
        }
        res.render("complete representation");
    }
}

fn service(origin: ConditionalOrigin) -> Service {
    Service::new(
        Router::new()
            .hoop(Cache::new(
                MokaStore::builder().build(),
                RequestIssuer::default(),
            ))
            .get(origin),
    )
}

#[tokio::test]
async fn warm_cache_preserves_origin_preconditions() {
    let calls = Arc::new(AtomicUsize::new(0));
    let service = service(ConditionalOrigin {
        calls: calls.clone(),
        started: None,
        release: None,
    });
    for _ in 0..2 {
        let mut response = TestClient::get("http://localhost/").send(&service).await;
        assert_eq!(
            response.take_string().await.unwrap(),
            "complete representation"
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    for (header, expected_status, expected_body) in [
        ("if-none-match", StatusCode::NOT_MODIFIED, ""),
        ("if-match", StatusCode::PRECONDITION_FAILED, ""),
        ("if-modified-since", StatusCode::OK, "if-modified-since"),
        ("if-unmodified-since", StatusCode::OK, "if-unmodified-since"),
        ("if-range", StatusCode::OK, "if-range"),
    ] {
        let mut response = TestClient::get("http://localhost/")
            .add_header(header, "validator", true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(expected_status), "{header}");
        if !expected_body.is_empty() {
            assert_eq!(response.take_string().await.unwrap(), expected_body);
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 6);
    let mut response = TestClient::get("http://localhost/").send(&service).await;
    assert_eq!(
        response.take_string().await.unwrap(),
        "complete representation"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 6);
}

#[tokio::test]
async fn conditional_request_does_not_join_unconditional_in_flight_request() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let service = Arc::new(service(ConditionalOrigin {
        calls: Arc::new(AtomicUsize::new(0)),
        started: Some(started.clone()),
        release: Some(release.clone()),
    }));
    let first_service = service.clone();
    let leader = tokio::spawn(async move {
        TestClient::get("http://localhost/")
            .send(&*first_service)
            .await
    });
    started.notified().await;
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        TestClient::get("http://localhost/")
            .add_header("if-none-match", "\"version\"", true)
            .send(&*service),
    )
    .await;
    release.notify_one();
    leader.await.unwrap();
    assert_eq!(
        response
            .expect("conditional request must not wait for cache fill")
            .status_code,
        Some(StatusCode::NOT_MODIFIED)
    );
}
