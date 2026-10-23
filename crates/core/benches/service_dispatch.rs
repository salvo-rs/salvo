#![allow(missing_docs)]
//! Benchmarks for full request dispatch through `Service`.
//!
//! Unlike `router_detect`, these cover the whole per-request pipeline:
//! routing, hoop-chain assembly, `FlowCtrl` execution, and response
//! rendering. Run with `cargo bench -p salvo_core --bench service_dispatch`.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use salvo_core::prelude::*;
use salvo_core::test::TestClient;

#[handler]
async fn hello() -> &'static str {
    "hello"
}

#[handler]
async fn noop_hoop() {}

fn bench_dispatch(c: &mut Criterion, name: &str, service: &Service, url: &str) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("build runtime");
    c.bench_function(name, |b| {
        b.iter(|| {
            let res = rt.block_on(async { TestClient::get(url).send(service).await });
            assert_eq!(res.status_code, Some(StatusCode::OK));
            black_box(res);
        });
    });
}

fn plain(c: &mut Criterion) {
    let service = Service::new(Router::new().push(Router::with_path("hello").get(hello)));
    bench_dispatch(c, "dispatch/plain", &service, "http://t.dev/hello");
}

fn hoop_chain(c: &mut Criterion) {
    // Eight no-op middleware on the matched branch: measures hoop-chain
    // assembly and FlowCtrl traversal overhead.
    let mut router = Router::with_path("hello");
    for _ in 0..8 {
        router = router.hoop(noop_hoop);
    }
    let service = Service::new(Router::new().push(router.get(hello)));
    bench_dispatch(c, "dispatch/hoops8", &service, "http://t.dev/hello");
}

fn dynamic_path(c: &mut Criterion) {
    let service =
        Service::new(Router::new().push(
            Router::with_path("users").push(
                Router::with_path("{id}").push(
                    Router::with_path("articles").push(Router::with_path("{aid}").get(hello)),
                ),
            ),
        ));
    bench_dispatch(
        c,
        "dispatch/dynamic_path",
        &service,
        "http://t.dev/users/12345/articles/67890",
    );
}

fn benches(c: &mut Criterion) {
    plain(c);
    hoop_chain(c);
    dynamic_path(c);
    negative_routes(c);
}

fn negative_routes(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("build runtime");
    let mut group = c.benchmark_group("dispatch/negative_routes");
    group.sample_size(10);
    for count in [100, 1_000, 10_000] {
        for multi in [false, true] {
            let mut router = Router::new();
            for i in 0..count {
                let mut route = Router::with_path(format!("route-{i}")).get(hello);
                if multi {
                    route = route.post(hello).put(hello);
                }
                router = router.push(route);
            }
            let service = Service::new(router);
            for (position, path, status) in [
                (
                    "first",
                    "route-0".to_owned(),
                    StatusCode::METHOD_NOT_ALLOWED,
                ),
                (
                    "last",
                    format!("route-{}", count - 1),
                    StatusCode::METHOD_NOT_ALLOWED,
                ),
                ("missing", "missing".to_owned(), StatusCode::NOT_FOUND),
            ] {
                let url = format!("http://t.dev/{path}");
                group.bench_function(format!("{count}/{multi}/{position}"), |b| {
                    b.iter(|| {
                        let res = rt.block_on(TestClient::delete(&url).send(&service));
                        assert_eq!(res.status_code, Some(status));
                        black_box(res);
                    })
                });
            }
        }
    }
    group.finish();
}

criterion_group!(service_dispatch, benches);
criterion_main!(service_dispatch);
