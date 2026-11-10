# cache_chunks benchmark

Run from the workspace root:

```text
cargo bench --locked -p salvo-cache --bench cache_chunks
```

Set SALVO_BENCH_SAMPLES to a positive count to change the measured iterations (default 128). The executable is also directly runnable after cargo build --release --bench cache_chunks -p salvo-cache. Custom harness: no nightly benchmark API or Criterion dependency.

The total body remains 64 KiB while chunk count changes (1/64/4096). entry_clone measures the public CachedEntry clone alone. hit routes a real GET through Cache/Moka and consumes every byte. cold_batch8 issues eight same-key misses together against a zero-capacity store, forces the handler to yield, verifies exactly one backend call per batch, and checks all eight complete bodies. Output for cold_batch8 is per batch, not per request; divide allocation/byte counts by eight for a batch-average request cost. The clone-only allocation-byte slope (32/2048/131072 in this run) is descriptor-container storage; it is not a deep copy of the 64 KiB payload. End-to-end rows include body collection and cache overhead.

## Measurement and interpretation

Each operation is timed with Instant; sorted samples use nearest-rank p50/p95/p99. Throughput is the reciprocal of total wall time including per-operation timing overhead. stats_alloc 0.1.10 instruments System without unsafe benchmark code. Allocation counts are process-wide regions and include runtime/cache background work; they are not an exact object-lifetime census. Atomic allocator instrumentation changes timings, so compare only identically instrumented builds. Sample buffers are preallocated before the region. Assertions remain enabled in release.

The checked-in CSV is one local Windows/Rust 1.99.1 run, with other development activity on the host; it records a baseline, not a controlled before/after speedup or a performance threshold. See the paired environment file. Repeated larger samples on a quiet host are needed before choosing an optimization. Linux CI pins Rust 1.95, records CPU/toolchain/commit/metadata/lockfile, builds first, and then measures the standalone executable under /usr/bin/time -v, preserving process CPU and peak RSS separately from compile cost. These Linux and Windows results must not be compared as if the environments matched. Process RSS is not per-response retained memory, and latency alone does not count syscalls or queue length.

Workflow: .github/workflows/perf-p03.yml. Its artifact contains samples.csv, resources.txt, the exact lockfile, toolchain/CPU/commit and build metadata even on failure. Results are advisory; this PR adds measurement rather than changing product behavior or claiming an unmeasured improvement.
