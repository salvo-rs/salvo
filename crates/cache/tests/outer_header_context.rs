//! Cache hits and coalesced followers must preserve the inner handler's header decisions.
#![allow(missing_docs, clippy::unwrap_used)]

use std::borrow::Borrow;
use std::convert::Infallible;
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use salvo_cache::{Cache, CacheStore, CachedEntry, MokaStore};
use salvo_core::prelude::*;
use salvo_core::test::TestClient;
use tokio::sync::{Notify, Semaphore};

#[handler]
async fn outer(req: &mut Request, res: &mut Response) {
    for (input, output) in [("x-outer-label", "x-label"), ("x-outer-id", "x-request-id")] {
        for value in req.headers().get_all(input) {
            res.headers_mut().append(output, value.clone());
        }
    }
}

#[derive(Debug)]
struct Page {
    calls: Arc<AtomicUsize>,
    remove: bool,
}

#[async_trait]
impl Handler for Page {
    async fn handle(&self, _: &mut Request, _: &mut Depot, res: &mut Response, _: &mut FlowCtrl) {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.remove {
            res.headers_mut().remove("x-label");
        } else {
            res.add_header("x-label", "fixed", true).unwrap();
        }
        res.render("body");
    }
}

fn key(_: &mut Request, _: &Depot) -> Option<String> {
    Some("one-key".into())
}

#[tokio::test]
async fn same_value_rewrites_and_fresh_request_ids_match_uncached_responses() {
    let calls = Arc::new(AtomicUsize::new(0));
    let router = Router::new()
        .hoop(outer)
        .hoop(Cache::new(MokaStore::new(10), key))
        .goal(Page {
            calls: calls.clone(),
            remove: false,
        });
    let service = Service::new(router);
    for (label, id, expected_calls) in [
        ("fixed", "one", 1),
        ("changed", "two", 2),
        ("changed", "two", 2),
    ] {
        let response = TestClient::get("http://localhost/")
            .add_header("x-outer-label", label, true)
            .add_header("x-outer-id", id, true)
            .send(&service)
            .await;
        assert_eq!(response.headers()["x-label"], "fixed");
        assert_eq!(response.headers()["x-request-id"], id);
        assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
    }
}

#[tokio::test]
async fn newly_added_outer_header_cannot_bypass_an_inner_deletion() {
    let calls = Arc::new(AtomicUsize::new(0));
    let router = Router::new()
        .hoop(outer)
        .hoop(Cache::new(MokaStore::new(10), key))
        .goal(Page {
            calls: calls.clone(),
            remove: true,
        });
    let service = Service::new(router);
    for label in [None, Some("new"), Some("new")] {
        let mut request = TestClient::get("http://localhost/");
        if let Some(label) = label {
            request = request.add_header("x-outer-label", label, true);
        }
        let response = request.send(&service).await;
        assert!(!response.headers().contains_key("x-label"));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[derive(Debug, Default)]
struct StoreInner {
    entry: Mutex<Option<CachedEntry>>,
    loads: AtomicUsize,
    follower_loaded: Notify,
}

#[derive(Clone, Debug, Default)]
struct Store(Arc<StoreInner>);

impl CacheStore for Store {
    type Error = Infallible;
    type Key = String;
    async fn load_entry<Q>(&self, _: &Q) -> Option<CachedEntry>
    where
        String: Borrow<Q>,
        Q: Hash + Eq + Sync,
    {
        if self.0.loads.fetch_add(1, Ordering::SeqCst) == 1 {
            self.0.follower_loaded.notify_one();
        }
        self.0.entry.lock().unwrap().clone()
    }
    async fn save_entry(&self, _: String, entry: CachedEntry) -> Result<(), Infallible> {
        *self.0.entry.lock().unwrap() = Some(entry);
        Ok(())
    }
}

#[derive(Debug)]
struct GatedPage {
    calls: Arc<AtomicUsize>,
    entered: Arc<Notify>,
    gate: Arc<Semaphore>,
}
#[async_trait]
impl Handler for GatedPage {
    async fn handle(&self, _: &mut Request, _: &mut Depot, res: &mut Response, _: &mut FlowCtrl) {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        self.gate.acquire().await.unwrap().forget();
        res.add_header("x-label", "fixed", true).unwrap();
        res.render("body");
    }
}

#[tokio::test]
async fn coalesced_follower_validates_its_own_outer_context() {
    let calls = Arc::new(AtomicUsize::new(0));
    let entered = Arc::new(Notify::new());
    let gate = Arc::new(Semaphore::new(0));
    let store = Store::default();
    let router = Arc::new(
        Router::new()
            .hoop(outer)
            .hoop(Cache::new(store.clone(), key))
            .goal(GatedPage {
                calls: calls.clone(),
                entered: entered.clone(),
                gate: gate.clone(),
            }),
    );
    let leader_router = router.clone();
    let leader = tokio::spawn(async move {
        TestClient::get("http://localhost/")
            .add_header("x-outer-label", "fixed", true)
            .send(leader_router)
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .unwrap();
    let follower = tokio::spawn(async move {
        TestClient::get("http://localhost/")
            .add_header("x-outer-label", "changed", true)
            .send(router)
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), store.0.follower_loaded.notified())
        .await
        .unwrap();
    gate.add_permits(1);
    assert_eq!(leader.await.unwrap().headers()["x-label"], "fixed");
    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .expect("a follower with a different header context must run the handler");
    gate.add_permits(1);
    assert_eq!(follower.await.unwrap().headers()["x-label"], "fixed");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
