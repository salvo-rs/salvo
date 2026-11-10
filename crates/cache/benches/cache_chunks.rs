#![allow(missing_docs, clippy::unwrap_used)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bytes::Bytes;
use futures_util::future::join_all;
use salvo_cache::{Cache, CachedBody, CachedEntry, MokaStore, RequestIssuer};
use salvo_core::http::{HeaderMap, ResBody};
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};

#[path = "support/cache_chunks.rs"]
mod support;

#[derive(Debug)]
struct Chunks {
    chunks: VecDeque<Bytes>,
    calls: Arc<AtomicUsize>,
}
#[async_trait]
impl Handler for Chunks {
    async fn handle(&self, _: &mut Request, _: &mut Depot, res: &mut Response, _: &mut FlowCtrl) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        // Give all requests in a cold batch a chance to join the same in-flight key.
        tokio::task::yield_now().await;
        res.add_header("content-type", "application/octet-stream", true)
            .unwrap();
        res.replace_body(ResBody::Chunks(self.chunks.clone()));
    }
}

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    support::header();
    let payload = Bytes::from(vec![42; 64 * 1024]);
    for count in [1, 64, 4096] {
        let chunks: VecDeque<_> = (0..count)
            .map(|index| {
                payload.slice(index * payload.len() / count..(index + 1) * payload.len() / count)
            })
            .collect();
        let entry = CachedEntry::new(
            Some(StatusCode::OK),
            HeaderMap::new(),
            CachedBody::Chunks(chunks.clone()),
        );
        support::measure(&runtime, &format!("entry_clone/{count}"), || async {
            std::hint::black_box(entry.clone());
        });
        for followers in [1, 8] {
            let calls = Arc::new(AtomicUsize::new(0));
            let service = Service::new(
                Router::with_path("{**path}")
                    .hoop(Cache::new(
                        MokaStore::new(if followers == 1 { 10 } else { 0 }),
                        RequestIssuer::default(),
                    ))
                    .get(Chunks {
                        chunks: chunks.clone(),
                        calls: calls.clone(),
                    }),
            );
            support::measure(
                &runtime,
                &format!(
                    "{}/{count}",
                    if followers == 1 { "hit" } else { "cold_batch8" }
                ),
                || async {
                    let before = calls.load(Ordering::Relaxed);
                    let responses = join_all(
                        (0..followers)
                            .map(|_| TestClient::get("http://localhost/chunks").send(&service)),
                    )
                    .await;
                    for mut response in responses {
                        assert_eq!(response.status_code, Some(StatusCode::OK));
                        assert_eq!(response.take_bytes(None).await.unwrap(), payload);
                    }
                    if followers == 8 {
                        assert_eq!(
                            calls.load(Ordering::Relaxed),
                            before + 1,
                            "misses must coalesce"
                        );
                    }
                },
            );
            if followers == 1 {
                assert_eq!(calls.load(Ordering::Relaxed), 1, "warm path must hit");
            }
        }
    }
}
