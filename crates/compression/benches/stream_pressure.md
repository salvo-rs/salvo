# Streaming compression and consumer pressure

Run `cargo bench --locked -p salvo-compression --bench stream_pressure`. SALVO_BENCH_SAMPLES is a positive count (default 16 plus 2 warmup batches). This matrix deliberately has 72 cases; increase sample counts for stable tail estimates.

A fixed 64 KiB mixed repetitive/nonrepetitive payload is exposed through a pull-counted public ResBody stream with terminal trailers. TestClient routes actual requests through Compression. The matrix crosses gzip/Brotli/Zstd, default/precise(5), 8/16/64 KiB input chunks (below/at/above the offload boundary), immediately ready/1 ms paused consumers, and 1/8 simultaneous responses. Tokio's blocking pool is fixed to four workers to make contention reproducible; this configures only the benchmark runtime, not production defaults.

For frame granularity, run `cargo bench --locked -p salvo-compression --bench
stream_pressure -- --granularity`. This independent 24-case mode holds input at
1 MiB, the consumer immediately ready, and concurrency at one, while crossing
64 B / 1 KiB / 16 KiB / 1 MiB frames with all three algorithms and both levels.
It exposes scheduling and per-frame flush costs without changing the historical
72-case default matrix or its checked-in CSV. Its timings still include decode
and assertions. Use `SALVO_BENCH_SAMPLES=1` only for a smoke measurement; one
sample cannot support tail-latency or speedup conclusions. Validate this mode
with `python3 crates/compression/benches/check_stream_pressure_csv.py
--granularity <csv-path>`. The workflow invokes it separately and preserves its
CSV and process resource report alongside the original matrix.

Every response validates status/negotiated encoding, complete decompression to the original bytes, trailers after all data, and source Drop. For paused consumers, the source poll counter must stay unchanged during each pause, checking pull-driven backpressure instead of inferring it from latency. Separate cancellation probes read one frame, drop the body, and assert source release and no further input reads. They do not claim that already-running spawn_blocking work can be aborted.

Rows are per batch. Multiply operations/second by concurrency to derive response throughput; allocations and bytes are per batch (divide by concurrency for the batch average). p50/p95/p99 measure the complete encode/consume/decode/assertion workload, and slow rows include consumer delay. Process CPU also includes decoding: these are not encoder-only CPU numbers. Encoded byte count and ratio are recorded separately to expose the flush/compression-ratio tradeoff.

stats_alloc instruments System without unsafe benchmark code. Counters are process-wide and include concurrent encoder workers. Preallocated timing samples exclude result-buffer allocation, but payload/body validation is included. Instrumentation overhead and host load affect timings. The local Windows/Rust 1.99.1 CSV is a short baseline, not a controlled before/after speedup or SLO. Queue depth is not directly measured; latency under the fixed four-worker budget is evidence to profile further, not a fabricated queue-length metric.

The SHA-pinned perf-p05 workflow builds with Rust 1.95, then measures the standalone executable under /usr/bin/time -v to separate compile cost from process CPU/peak RSS. Exact commit, lockfile, CPU, toolchain, build metadata and results are always uploaded. No production thresholds, global compression budget or algorithm changes are introduced without this measurement foundation.
