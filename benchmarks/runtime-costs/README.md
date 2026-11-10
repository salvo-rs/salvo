# Runtime and feature costs (P06)

This workload measures the aggregate cost of the correctness changes between
`e29dbf75` (before the original six fixes) and the selected current revision.
It adds no production optimization or latency threshold.

From the repository root, with Python 3.11+, Git, Cargo and a supported linker:

```sh
python -m unittest discover -s benchmarks -p 'test_*.py'
python benchmarks/measure_runtime_costs.py --jobs 2 --samples 128
```

The default current revision is `HEAD`; `--baseline`, `--current`,
`--skip-feature-costs` and `--output target/another-comparison` support reruns.
Executables are discovered from Cargo's JSON `compiler-artifact.executable`
messages for both runtime execution and feature binary sizing. The runner inherits
configured build targets (including `CARGO_BUILD_TARGET`); it does not assume a
`target/release` layout or add a target override. Regression cases include stale
default-layout binaries next to configured-target artifacts. CI additionally sets
its native `CARGO_BUILD_TARGET` for a separate smoke step that builds and runs
both real fixtures; the smoke script inherits that setting without overriding it.

The output directory must stay in this checkout. The runner creates detached
before/after worktrees inside it, copies the **same fixture** into both, resolves
one dependency lock and builds both with its external dependencies pinned. Local
path package entries are refreshed offline to match each revision's versions;
any external dependency change stops the comparison. It also stops if that lock
cannot build the selected revision, rather than silently changing dependencies.
Use a compatible revision pair or explicitly update the experiment in that case.
An existing worktree at a different revision is never reset or overwritten;
choose a new output directory. Evidence includes fixture hashes, full commit IDs,
locks, metadata, build logs, raw CSV, toolchain and Linux `/usr/bin/time -v` output.
Worktrees and their build directories remain available for inspection.

Every fixture uses Cargo metadata to discover its normal target. The script does
not change Cargo home, target, TEMP/TMP or the caller's environment, except for
the benchmark sample count. Feature profiling invokes `cargo clean` **only** when
metadata resolves to that isolated fixture's default `target` directory; custom
or shared targets are rejected before cleaning. The safety tests cover those
refusals, checkout reuse and failed-command logs.

## Runtime matrix and correctness

The 15 cases use real public APIs:

- Plain and generic `Extractible::metadata()` warm lookups in batches of 1,000.
  The derived generic flatten extractor first parses and checks an actual query.
- The same metadata lookups from four scoped OS threads, each performing 10,000
  lookups, to expose lock contention. Thread creation/join is included; the plain
  case is a matched control, not a subtraction of scheduler cost.
- Plain `Service` and a Tower identity layer through `TowerLayerCompat`, with an
  actual TestClient request and exact status/body checks.
- Loopback TCP HTTP/1 fresh and persistent connections under disabled, immediate,
  and once-yielding admission policies. Each policy accepts with disabled fuse
  limits so this measures policy/future and task-scope overhead without timeouts
  from fuse limits. Operations check HTTP 200, explicit two-byte Content-Length,
  exact body and EOF for close requests, with a five-second liveness bound.
- Each policy also runs a batch of eight concurrent requests: four existing
  persistent sockets and four fresh sockets, distributed over plain, generic
  query extraction and Tower routes. Every response is checked. After each case
  the server is gracefully stopped and its task result is checked.

Eight warmups precede 128 measured operations. Nearest-rank p50/p95/p99, operation
rate and global allocator allocations/bytes are emitted. A metadata operation is
1,000 or 40,000 lookups; a mixed operation is **eight requests**, so divide its
batch latency/allocations by eight only when discussing amortized per-request
cost (its batch quantiles are not individual-request quantiles).

The client, parser, server and assertions share one current-thread Tokio runtime;
only metadata contention uses OS threads. Fresh connections include connect and
close, persistent ones exclude connection setup, and mixed batches include task
creation. Measurements include instrumentation and client cost. This is bounded
local HTTP/1 concurrency, not remote networking, TLS, HTTP/2, production capacity,
or isolated causal attribution to any one fix. Request payloads are deliberately
small to expose overhead; P01/P03/P05 cover filesystem, cached bodies and
compression separately.

## Feature build matrix

`benchmarks/feature-costs` is a standalone downstream consumer. The minimal mode
uses `default-features=false` with server/http1; full adds `salvo/full`. For each
mode, the runner records a clean **target** release build, an unchanged
incremental build, Cargo timing HTML, unique normal/build package-version count
(including the consumer), lock/tree, and executable size. Registry downloads,
OS caches and compiler installation are not cleared. The smoke binary constructs
a Router, so linker dead stripping means its size is not a representative
full-feature application size. Build costs include compilation of enabled
libraries even when the smoke binary does not call them.

CI uses Ubuntu 24.04/Rust 1.95.0, runs the complete comparison and feature matrix,
and archives evidence for 30 days. It excludes the nested revision worktrees and
their large targets from uploads. Shared-runner timing is informational; correctness
or profiling failures fail CI, noisy throughput alone does not.

## Checked-in baseline

See `results/` for a short Windows x86_64 / Rust 1.99.1 run on an Intel Core
i7-14700KF (28 logical CPUs). Its identical fixture hashes and two revision IDs
are recorded alongside raw samples. Local builds used `-j 2`; other agents were
also compiling, so timing and especially connection tails are noisy. These values
are a reproducibility example, not a speedup claim or a regression budget.

One signal deserves follow-up rather than a blanket optimization: warm generic
metadata p50 for 1,000 lookups changed from 0.3 us to 14.4 us; the four-thread
40,000-lookup batch changed from 0.135 ms to 4.224 ms. Plain controls and other
request timings also varied with host load. The generic cache now preserves
per-instantiation correctness; any future faster cache must keep that property.
The TCP/Tower comparisons should be repeated on an otherwise idle machine before
setting budgets. No correctness fix is reverted on the basis of this short run.

The same local feature run recorded the following command wall times (including
cache-lock waits and process overhead), package counts and smoke binary sizes:

| Mode | Clean-target build | Unchanged build | Packages | Executable bytes |
| --- | ---: | ---: | ---: | ---: |
| minimal | 44.35 s | 0.21 s | 109 | 126464 |
| full | 388.34 s | 20.27 s | 393 | 1654272 |

The full unchanged build waited on the package-cache lock: Cargo itself reported
0.68 s after that wait, while the complete command took 20.27 s. Run the matrix
on an idle host before using these timings for a build budget. The `runtime.lock` (shared by both revisions), `features.lock` (shared by both
feature modes) and SHA-256 records preserve this local dependency resolution; CI also archives
its own locks, logs and Cargo timing HTML.
