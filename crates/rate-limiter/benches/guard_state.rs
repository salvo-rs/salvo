#![allow(missing_docs, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use salvo_rate_limiter::{
    BasicQuota, CelledQuota, FixedGuard, MokaStore, RateGuard, RateStore, SlidingGuard,
};

#[path = "support/guard_state.rs"]
mod support;

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    support::header();
    let quota = BasicQuota::per_hour(1_000_000);
    for hot in [true, false] {
        let store = MokaStore::<usize, FixedGuard>::with_max_capacity(100_000);
        let counter = AtomicUsize::new(0);
        let reference = FixedGuard::new();
        support::measure(
            &runtime,
            &format!("fixed/{}", if hot { "hot" } else { "unique" }),
            || async {
                let sequence = counter.fetch_add(1, Ordering::Relaxed);
                let state = store
                    .verify_guard(if hot { 0 } else { sequence }, &reference, &quota)
                    .await
                    .unwrap();
                assert!(state.allowed);
                assert_eq!(
                    state.guard.remaining(&quota).await,
                    1_000_000 - if hot { sequence + 1 } else { 1 }
                );
                std::hint::black_box(state);
            },
        );
    }
    for cells in [1, 64, 1024] {
        let quota = CelledQuota::per_hour(1_000_000, cells);
        let mut initialized = SlidingGuard::new();
        assert!(runtime.block_on(initialized.verify(&quota)));
        support::measure(&runtime, &format!("sliding_clone/{cells}"), || async {
            std::hint::black_box(initialized.clone());
        });
        for hot in [true, false] {
            let store = MokaStore::<usize, SlidingGuard>::with_max_capacity(100_000);
            let counter = AtomicUsize::new(0);
            let reference = SlidingGuard::new();
            support::measure(
                &runtime,
                &format!("sliding/{cells}/{}", if hot { "hot" } else { "unique" }),
                || async {
                    let sequence = counter.fetch_add(1, Ordering::Relaxed);
                    let state = store
                        .verify_guard(if hot { 0 } else { sequence }, &reference, &quota)
                        .await
                        .unwrap();
                    assert!(state.allowed);
                    assert_eq!(
                        state.guard.remaining(&quota).await,
                        1_000_000 - if hot { sequence + 1 } else { 1 }
                    );
                    std::hint::black_box(state);
                },
            );
        }
    }
}
