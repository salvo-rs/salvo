# static_roots benchmark

Run from the workspace root:

```text
cargo bench --locked -p salvo-serve-static --bench static_roots
```

Set SALVO_BENCH_SAMPLES to a positive count to change the measured iterations (default 64). The executable is also directly runnable after cargo build --release --bench static_roots -p salvo-serve-static. Custom harness: no nightly benchmark API or Criterion dependency.

The matrix keeps one 3.75 KiB file representation fixed and varies root count (1/4/16), first/last hit, miss, and a gzip sidecar in the last root. Every iteration consumes the entire response, checks status, and compares raw representation bytes; the sidecar also checks Content-Encoding. Dot-file rejection is checked outside timing. Setup, file creation, compression of the sidecar, and eight warmups are excluded. The timings include Service/TestClient, file reading, body collection and assertions, not a socket transport. Filesystem caches are warm. No filesystem-call count is inferred from latency; the Linux workflow separately records aggregate strace -c -f counts around the standalone executable. This trace includes fixture setup and warmup and is not a per-request syscall count; its instrumented timings are discarded.

## Measurement and interpretation

Each operation is timed with Instant; sorted samples use nearest-rank p50/p95/p99. Throughput is the reciprocal of total wall time including per-operation timing overhead. stats_alloc 0.1.10 instruments System without unsafe benchmark code. Allocation counts are process-wide regions and include runtime/cache background work; they are not an exact object-lifetime census. Atomic allocator instrumentation changes timings, so compare only identically instrumented builds. Sample buffers are preallocated before the region. Assertions remain enabled in release.

The checked-in CSV is one local Windows/Rust 1.99.1 run, with other development activity on the host; it records a baseline, not a controlled before/after speedup or a performance threshold. See the paired environment file. Repeated larger samples on a quiet host are needed before choosing an optimization. Linux CI pins Rust 1.95, records CPU/toolchain/commit/metadata/lockfile, builds first, and then measures the standalone executable under /usr/bin/time -v, preserving process CPU and peak RSS separately from compile cost. These Linux and Windows results must not be compared as if the environments matched. Process RSS is not per-response retained memory, and latency alone does not count syscalls or queue length.

Workflow: .github/workflows/perf-p01.yml. Its artifact contains samples.csv, resources.txt, the exact lockfile, toolchain/CPU/commit and build metadata even on failure. Results are advisory; this PR adds measurement rather than changing product behavior or claiming an unmeasured improvement.
