# guard_state benchmark

Run from the workspace root:

```text
cargo bench --locked -p salvo-rate-limiter --bench guard_state
```

Set SALVO_BENCH_SAMPLES to a positive count to change the measured iterations (default 128). The executable is also directly runnable after cargo build --release --bench guard_state -p salvo-rate-limiter. Custom harness: no nightly benchmark API or Criterion dependency.

The public RateStore::verify_guard path compares FixedGuard with SlidingGuard at 1/64/1024 cells, using one hot key and unique keys. A one-hour, 1,000,000-request quota avoids window rollover or rejection in this short run. Every operation checks allowed and exact remaining quota. Clone-only rows initialize the real SlidingGuard first, then clone it; measured allocation bytes of 8/512/8192 track its usize bucket vector. Store rows include Moka synchronization, expiry, guard cloning and atomic bookkeeping. They do not justify replacing atomic same-key updates with load-modify-save. Unique-key runs measure admission/allocation without capacity eviction.

## Measurement and interpretation

Each operation is timed with Instant; sorted samples use nearest-rank p50/p95/p99. Throughput is the reciprocal of total wall time including per-operation timing overhead. stats_alloc 0.1.10 instruments System without unsafe benchmark code. Allocation counts are process-wide regions and include runtime/cache background work; they are not an exact object-lifetime census. Atomic allocator instrumentation changes timings, so compare only identically instrumented builds. Sample buffers are preallocated before the region. Assertions remain enabled in release.

The checked-in CSV is one local Windows/Rust 1.99.1 run, with other development activity on the host; it records a baseline, not a controlled before/after speedup or a performance threshold. See the paired environment file. Repeated larger samples on a quiet host are needed before choosing an optimization. Linux CI pins Rust 1.95, records CPU/toolchain/commit/metadata/lockfile, builds first, and then measures the standalone executable under /usr/bin/time -v, preserving process CPU and peak RSS separately from compile cost. These Linux and Windows results must not be compared as if the environments matched. Process RSS is not per-response retained memory, and latency alone does not count syscalls or queue length.

Workflow: .github/workflows/perf-p04.yml. Its artifact contains samples.csv, resources.txt, the exact lockfile, toolchain/CPU/commit and build metadata even on failure. Results are advisory; this PR adds measurement rather than changing product behavior or claiming an unmeasured improvement.
