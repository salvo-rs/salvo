#![allow(missing_docs, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use salvo_cache::{Cache, MokaStore, RequestIssuer};
use salvo_core::http::header::HeaderName;
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};

#[path = "support/cache_headers.rs"]
mod support;

#[derive(Debug)]
struct Outer(Vec<HeaderName>);
#[async_trait]
impl Handler for Outer {
    async fn handle(&self, _: &mut Request, _: &mut Depot, res: &mut Response, _: &mut FlowCtrl) {
        for name in &self.0 {
            res.add_header(name, "request-policy", true).unwrap();
        }
    }
}

#[derive(Debug)]
struct Inner {
    headers: Vec<HeaderName>,
    calls: Arc<AtomicUsize>,
}
#[async_trait]
impl Handler for Inner {
    async fn handle(&self, _: &mut Request, _: &mut Depot, res: &mut Response, _: &mut FlowCtrl) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        for name in &self.headers {
            res.add_header(name, "first", false).unwrap();
            res.add_header(name, "second", false).unwrap();
        }
        res.render("response");
    }
}

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    support::header();
    for headers in [8, 32, 128] {
        for hit in [false, true] {
            let calls = Arc::new(AtomicUsize::new(0));
            let cache = Cache::new(
                MokaStore::new(if hit { 10 } else { 0 }),
                RequestIssuer::default(),
            );
            let service = Service::new(
                Router::with_path("{**path}")
                    .hoop(Outer(
                        (0..headers)
                            .map(|i| {
                                HeaderName::from_bytes(format!("x-outer-{i}").as_bytes()).unwrap()
                            })
                            .collect(),
                    ))
                    .hoop(cache)
                    .get(Inner {
                        headers: (0..headers)
                            .map(|i| {
                                HeaderName::from_bytes(format!("x-inner-{i}").as_bytes()).unwrap()
                            })
                            .collect(),
                        calls: calls.clone(),
                    }),
            );
            support::measure(
                &runtime,
                &format!("{}/{headers}", if hit { "hit" } else { "miss" }),
                || async {
                    let before = calls.load(Ordering::Relaxed);
                    let mut response = TestClient::get("http://localhost/headers")
                        .send(&service)
                        .await;
                    assert_eq!(response.status_code, Some(StatusCode::OK));
                    assert_eq!(response.headers().get_all("x-inner-0").iter().count(), 2);
                    assert_eq!(response.headers()["x-outer-0"], "request-policy");
                    assert_eq!(response.take_string().await.unwrap(), "response");
                    if !hit {
                        assert_eq!(calls.load(Ordering::Relaxed), before + 1);
                    }
                },
            );
            if hit {
                assert_eq!(
                    calls.load(Ordering::Relaxed),
                    1,
                    "warm path must actually hit"
                );
            }
        }
    }
}
