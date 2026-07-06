#![allow(missing_docs)]
//! Benchmarks for the routing hot path: `Router::detect`.
//!
//! Scenarios cover the shapes that stress different parts of the matcher:
//! flat static routes, dynamic params, deep nesting, wide sibling fan-out
//! (worst case for the linear scan), and wildcard tails. Run with
//! `cargo bench -p salvo_core --bench router_detect`.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use salvo_core::routing::PathState;
use salvo_core::test::TestClient;
use salvo_core::{Router, handler};

#[handler]
async fn goal() -> &'static str {
    "ok"
}

fn bench_detect(c: &mut Criterion, name: &str, router: &Router, url: &str) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("build runtime");
    let mut req = TestClient::get(url).build();
    let path = req.uri().path().to_owned();
    c.bench_function(name, |b| {
        b.iter(|| {
            let mut state = PathState::from_borrowed_path(&path);
            let matched = rt.block_on(router.detect(&mut req, &mut state));
            assert!(matched.is_some(), "route must match in benchmark");
            black_box(state);
        });
    });
}

fn static_shallow(c: &mut Criterion) {
    let router = Router::new()
        .push(Router::with_path("users").goal(goal))
        .push(Router::with_path("articles").goal(goal))
        .push(Router::with_path("health").goal(goal));
    bench_detect(c, "detect/static_shallow", &router, "http://t.dev/health");
}

fn dynamic_params(c: &mut Criterion) {
    let router = Router::new().push(
        Router::with_path("users").push(
            Router::with_path("{id}")
                .push(Router::with_path("articles").push(Router::with_path("{aid}").goal(goal))),
        ),
    );
    bench_detect(
        c,
        "detect/dynamic_params",
        &router,
        "http://t.dev/users/12345/articles/67890",
    );
}

fn deep_tree(c: &mut Criterion) {
    // Eight nested levels alternating static and param segments.
    let mut leaf = Router::with_path("leaf").goal(goal);
    for level in (0..8).rev() {
        let seg = if level % 2 == 0 {
            format!("level{level}")
        } else {
            format!("{{p{level}}}")
        };
        leaf = Router::with_path(seg).push(leaf);
    }
    let router = Router::new().push(leaf);
    bench_detect(
        c,
        "detect/deep_tree",
        &router,
        "http://t.dev/level0/v1/level2/v3/level4/v5/level6/v7/leaf",
    );
}

fn wide_siblings(c: &mut Criterion) {
    // 100 sibling routes under one parent; the request matches the last one,
    // which is the worst case for the linear sibling scan.
    let mut parent = Router::with_path("api");
    for i in 0..100 {
        parent = parent.push(Router::with_path(format!("res{i:03}")).goal(goal));
    }
    let router = Router::new().push(parent);
    bench_detect(
        c,
        "detect/wide_siblings_last",
        &router,
        "http://t.dev/api/res099",
    );
}

fn wildcard_tail(c: &mut Criterion) {
    let router = Router::new().push(Router::with_path("assets/{**rest}").goal(goal));
    bench_detect(
        c,
        "detect/wildcard_tail",
        &router,
        "http://t.dev/assets/css/site/theme/main.css",
    );
}

fn sibling_param_backtrack(c: &mut Criterion) {
    // Earlier siblings capture params and then fail on a deeper segment,
    // exercising the snapshot/rollback path before the last sibling matches.
    let router = Router::new().push(
        Router::with_path("users")
            .push(Router::with_path("{id}/profile").goal(goal))
            .push(Router::with_path("{id}/settings").goal(goal))
            .push(Router::with_path("{name}").goal(goal)),
    );
    bench_detect(
        c,
        "detect/sibling_param_backtrack",
        &router,
        "http://t.dev/users/alice",
    );
}

fn benches(c: &mut Criterion) {
    static_shallow(c);
    dynamic_params(c);
    deep_tree(c);
    wide_siblings(c);
    wildcard_tail(c);
    sibling_param_backtrack(c);
}

criterion_group!(router_detect, benches);
criterion_main!(router_detect);
