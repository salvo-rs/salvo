#![allow(missing_docs, clippy::unwrap_used)]

use std::alloc::System;
use std::io::Write;
use std::time::Instant;

use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_serve_static::StaticDir;
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn main() {
    let samples: usize =
        std::env::var("SALVO_BENCH_SAMPLES").map_or(64, |v| v.parse().expect("sample count"));
    assert!(samples > 0);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let text = "static payload\n".repeat(256);
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(text.as_bytes()).unwrap();
    let gzip = encoder.finish().unwrap();
    println!(
        "roots,position,samples,p50_ns,p95_ns,p99_ns,requests_per_second,allocations_per_request,allocated_bytes_per_request"
    );
    for count in [1, 4, 16] {
        let directory = tempfile::tempdir().unwrap();
        let roots: Vec<_> = (0..count)
            .map(|i| directory.path().join(i.to_string()))
            .collect();
        for root in &roots {
            std::fs::create_dir(root).unwrap();
        }
        std::fs::write(roots[0].join("first.txt"), &text).unwrap();
        std::fs::write(roots[count - 1].join("last.txt"), &text).unwrap();
        std::fs::write(roots[count - 1].join("last.txt.gz"), &gzip).unwrap();
        std::fs::write(roots[0].join(".hidden"), "private").unwrap();
        let service = Service::new(Router::with_path("{**path}").goal(StaticDir::new(roots)));
        for (position, path, encoded, expected) in [
            ("first", "first.txt", false, text.as_bytes()),
            ("last", "last.txt", false, text.as_bytes()),
            ("missing", "missing.txt", false, &[][..]),
            ("sidecar_last", "last.txt", true, gzip.as_slice()),
        ] {
            let operation = || async {
                let mut request = TestClient::get(format!("http://localhost/{path}"));
                if encoded {
                    request = request.add_header("accept-encoding", "gzip", true);
                }
                let mut response = request.send(&service).await;
                assert_eq!(
                    response.status_code,
                    Some(if position == "missing" {
                        StatusCode::NOT_FOUND
                    } else {
                        StatusCode::OK
                    })
                );
                if encoded {
                    assert_eq!(response.headers()["content-encoding"], "gzip");
                }
                let body = response.take_bytes(None).await.unwrap();
                if position != "missing" {
                    assert_eq!(body.as_ref(), expected);
                }
                std::hint::black_box(body);
            };
            for _ in 0..8 {
                runtime.block_on(operation());
            }
            let mut durations = Vec::with_capacity(samples);
            let allocations = Region::new(ALLOCATOR);
            let wall = Instant::now();
            for _ in 0..samples {
                let started = Instant::now();
                runtime.block_on(operation());
                durations.push(started.elapsed().as_nanos());
            }
            let elapsed = wall.elapsed().as_secs_f64();
            let stats = allocations.change();
            durations.sort_unstable();
            let quantile =
                |percent: usize| durations[(samples * percent).div_ceil(100).saturating_sub(1)];
            println!(
                "{count},{position},{samples},{},{},{},{:.1},{:.2},{:.2}",
                quantile(50),
                quantile(95),
                quantile(99),
                samples as f64 / elapsed,
                stats.allocations as f64 / samples as f64,
                stats.bytes_allocated as f64 / samples as f64
            );
        }
        runtime.block_on(async {
            let response = TestClient::get("http://localhost/.hidden")
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::NOT_FOUND));
        });
    }
}
