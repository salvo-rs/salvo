# Cache header benchmark

Run `cargo bench --locked -p salvo-cache --bench cache_headers` from the workspace root. SALVO_BENCH_SAMPLES controls the positive sample count (default 128, plus 8 warmups).

The real Service/Cache/Moka pipeline measures misses and warmed hits with 8/32/128 outer headers and the same number of two-valued inner headers. Names are parsed once before measurement. The handler returns a fixed body; each operation checks both header ownership classes, duplicate values, status and full body. Zero store capacity forces the miss cases; an origin counter verifies every miss invokes the handler and each entire warm case invokes it only once. This is an end-to-end header-rich pipeline, not a duplicated implementation of the private comparison function.

Two checked-in CSV files retain the original product baseline and an additional run applying R03 commit 7f370550's runtime header-context change. Both validate the same public contract. Outer values are intentionally stable so guarded cache reuse remains legal; this benchmark does not assume that changing an outer header must remain a hit. R03's context checks have their own allocation cost. These short Windows runs are not a controlled speedup claim and do not change response-header semantics.

Output reports nearest-rank p50/p95/p99, total wall throughput, and process-wide stats_alloc System allocation/byte counts per operation. Timing includes request construction, handlers, body collection and assertions; allocator instrumentation changes timing, and Moka background work may contribute allocations. Compare only identical environments and repeat larger samples before choosing an optimization. No automatic comparison-function optimization is included.

The Linux perf-p02 workflow pins Rust 1.95, builds before runtime measurement, records CPU/toolchain/commit/metadata/lockfile, and preserves process CPU, peak RSS and CSV with always() artifact upload. Windows/Rust 1.99.1 baseline details are in results/cache_headers-environment.txt. Runtime RSS is a process metric, not the retained size of an individual cache entry.
