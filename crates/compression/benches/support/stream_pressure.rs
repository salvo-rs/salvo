#![allow(dead_code, clippy::unwrap_used)]

use std::alloc::System;
use std::time::Instant;

use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

pub(super) fn samples() -> usize {
    let count =
        std::env::var("SALVO_BENCH_SAMPLES").map_or(16, |v| v.parse().expect("sample count"));
    assert!(count > 0);
    count
}

pub(super) fn header() {
    println!(
        "case,samples,p50_ns,p95_ns,p99_ns,operations_per_second,allocations_per_operation,allocated_bytes_per_operation,encoded_bytes,input_bytes,compression_ratio"
    );
}

pub(super) fn measure<F, T>(
    runtime: &tokio::runtime::Runtime,
    label: &str,
    encoded_bytes: usize,
    mut operation: F,
) where
    F: FnMut() -> T,
    T: Future<Output = ()>,
{
    for _ in 0..2 {
        runtime.block_on(operation());
    }
    let count = samples();
    let mut durations = Vec::with_capacity(count);
    let allocations = Region::new(ALLOCATOR);
    let started = Instant::now();
    for _ in 0..count {
        let iteration = Instant::now();
        runtime.block_on(operation());
        durations.push(iteration.elapsed().as_nanos());
    }
    let elapsed = started.elapsed().as_secs_f64();
    let stats = allocations.change();
    durations.sort_unstable();
    let quantile = |p: usize| durations[(count * p).div_ceil(100).saturating_sub(1)];
    println!(
        "{label},{count},{},{},{},{:.1},{:.2},{:.2},{encoded_bytes},65536,{:.4}",
        quantile(50),
        quantile(95),
        quantile(99),
        count as f64 / elapsed,
        stats.allocations as f64 / count as f64,
        stats.bytes_allocated as f64 / count as f64,
        encoded_bytes as f64 / 65536.0
    );
}
