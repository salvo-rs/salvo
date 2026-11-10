//! Request bypass directives must not read, write, or join an in-flight cache entry.
#![allow(missing_docs, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use salvo_cache::{Cache, MokaStore, RequestIssuer};
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};

#[derive(Debug)]
struct Page(Arc<AtomicUsize>);
#[async_trait]
impl Handler for Page {
    async fn handle(&self, _: &mut Request, _: &mut Depot, res: &mut Response, _: &mut FlowCtrl) {
        res.render(self.0.fetch_add(1, Ordering::SeqCst).to_string());
    }
}

#[tokio::test]
async fn bypass_directives_read_fresh_without_replacing_an_existing_entry() {
    for directive in [
        "no-cache",
        "NO-STORE",
        "max-age=0",
        "max-age=600",
        "min-fresh=10",
        "no-cache=\"x-tag\"",
    ] {
        let calls = Arc::new(AtomicUsize::new(0));
        let service = Service::new(
            Router::new()
                .hoop(Cache::new(MokaStore::new(10), RequestIssuer::new()))
                .goal(Page(calls.clone())),
        );
        let mut warm = TestClient::get("http://localhost/").send(&service).await;
        assert_eq!(warm.take_string().await.unwrap(), "0");
        let mut fresh = TestClient::get("http://localhost/")
            .add_header("cache-control", "extension=\"a,b\"", true)
            .add_header("cache-control", directive, false)
            .send(&service)
            .await;
        assert_eq!(fresh.take_string().await.unwrap(), "1", "{directive}");
        let mut hit = TestClient::get("http://localhost/").send(&service).await;
        assert_eq!(hit.take_string().await.unwrap(), "0", "{directive}");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn no_store_does_not_populate_a_cold_cache() {
    let calls = Arc::new(AtomicUsize::new(0));
    let service = Service::new(
        Router::new()
            .hoop(Cache::new(MokaStore::new(10), RequestIssuer::new()))
            .goal(Page(calls.clone())),
    );
    let mut first = TestClient::get("http://localhost/")
        .add_header("cache-control", "no-store", true)
        .send(&service)
        .await;
    assert_eq!(first.take_string().await.unwrap(), "0");
    for _ in 0..2 {
        let mut regular = TestClient::get("http://localhost/").send(&service).await;
        assert_eq!(regular.take_string().await.unwrap(), "1");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let mut legacy = TestClient::get("http://localhost/")
        .add_header("pragma", "no-cache", true)
        .send(&service)
        .await;
    assert_eq!(legacy.take_string().await.unwrap(), "2");
}

#[tokio::test]
async fn unrelated_and_quoted_extension_tokens_do_not_disable_memoization() {
    let calls = Arc::new(AtomicUsize::new(0));
    let service = Service::new(
        Router::new()
            .hoop(Cache::new(MokaStore::new(10), RequestIssuer::new()))
            .goal(Page(calls.clone())),
    );
    for _ in 0..2 {
        let mut response = TestClient::get("http://localhost/")
            .add_header("cache-control", "extension=\"no-store,no-cache\"", true)
            .send(&service)
            .await;
        assert_eq!(response.take_string().await.unwrap(), "0");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[derive(Clone, Debug)]
struct GatedPage {
    calls: Arc<AtomicUsize>,
    entered: Arc<tokio::sync::Notify>,
    gate: Arc<tokio::sync::Semaphore>,
}
#[async_trait]
impl Handler for GatedPage {
    async fn handle(&self, _: &mut Request, _: &mut Depot, res: &mut Response, _: &mut FlowCtrl) {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            self.entered.notify_one();
            self.gate.acquire().await.unwrap().forget();
        }
        res.render(call.to_string());
    }
}

#[tokio::test]
async fn bypass_request_does_not_join_an_in_flight_population() {
    let page = GatedPage {
        calls: Arc::new(AtomicUsize::new(0)),
        entered: Arc::new(tokio::sync::Notify::new()),
        gate: Arc::new(tokio::sync::Semaphore::new(0)),
    };
    let service = Arc::new(
        Router::new()
            .hoop(Cache::new(MokaStore::new(10), RequestIssuer::new()))
            .goal(page.clone()),
    );
    let first_service = service.clone();
    let first = tokio::spawn(async move {
        TestClient::get("http://localhost/")
            .send(first_service)
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), page.entered.notified())
        .await
        .unwrap();
    let mut bypass = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        TestClient::get("http://localhost/")
            .add_header("cache-control", "no-store", true)
            .send(service.clone()),
    )
    .await
    .expect("no-store must not wait for the normal cache population");
    assert_eq!(bypass.take_string().await.unwrap(), "1");
    page.gate.add_permits(1);
    let mut warm = first.await.unwrap();
    assert_eq!(warm.take_string().await.unwrap(), "0");
    let mut hit = TestClient::get("http://localhost/").send(service).await;
    assert_eq!(hit.take_string().await.unwrap(), "0");
}
