#![allow(missing_docs)]
//! End-to-end buffered ETag benchmark. Run with:
//! `cargo bench -p salvo_extra --bench buffered_etag`.
//! Reports per-request time including routing and response construction.

use std::hint::black_box;
use std::sync::LazyLock;
use std::time::Instant;

use bytes::Bytes;
use salvo_core::http::ResBody;
use salvo_core::prelude::*;
use salvo_core::test::TestClient;
use salvo_extra::caching_headers::ETag;

static PAYLOAD: LazyLock<Bytes> = LazyLock::new(|| Bytes::from(vec![b'x'; 8 * 1024 * 1024]));

#[handler]
async fn buffered(req: &mut Request, res: &mut Response) {
    let len = req.query::<usize>("len").unwrap();
    let chunk = req.query::<usize>("chunk").unwrap();
    let bytes = PAYLOAD.slice(..len);
    if chunk == 0 {
        res.body(ResBody::Once(bytes));
    } else {
        res.body(ResBody::Chunks(
            (0..len)
                .step_by(chunk)
                .map(|start| bytes.slice(start..(start + chunk).min(len)))
                .collect(),
        ));
    }
}

fn main() {
    LazyLock::force(&PAYLOAD);
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let service = Service::new(Router::with_hoop(ETag::new()).get(buffered));
    for len in [64 * 1024, 8 * 1024 * 1024] {
        for chunk in [0, 1024, 64 * 1024] {
            let url = format!("http://localhost/?len={len}&chunk={chunk}");
            let iterations = if len > 1024 * 1024 { 100 } else { 1000 };
            let run = || {
                let res = rt.block_on(TestClient::get(&url).send(&service));
                assert_eq!(res.status_code, Some(StatusCode::OK));
                assert!(res.headers().contains_key("etag"));
                black_box(res);
            };
            for _ in 0..10 {
                run();
            }
            let start = Instant::now();
            for _ in 0..iterations {
                run();
            }
            println!(
                "bytes={len}, chunk={chunk} (0=Once): {:?}/request ({iterations} samples)",
                start.elapsed() / iterations
            );
        }
    }
}
