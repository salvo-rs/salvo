# HTTP server executable sizes

This standalone workspace compares deployed HTTP/1 servers built with Salvo and
axum. Each runs a current-thread Tokio runtime, binds an ephemeral loopback port,
and serves the exact `Hello World` body through its framework Router. Neither
consumer enables default framework features, TLS, HTTP/2, logging subscribers or
application middleware. The shared Cargo.lock pins common dependency versions;
framework-specific dependency features remain each framework's own minimum.

From the repository root, with Python 3.11+, Cargo and a native Rust toolchain:

```sh
python -m unittest discover -s benchmarks -p 'test_measure_http_server_size.py'
python benchmarks/measure_http_server_size.py --jobs 2
```

The runner builds eight cases: both frameworks, root versus `/hello` routes, and
ordinary release versus the `size` profile. The latter sets `opt-level=z`, fat
LTO, one codegen unit, `panic=abort` and symbol stripping. These settings affect
panic recovery and may affect runtime performance; this experiment measures
executable size, not throughput or latency. Defaults/full-feature applications
are outside this comparison. Recorded build times are diagnostic wall times;
cases reuse dependency artifacts and are not cold-build performance comparisons.

Each executable announces its dynamically assigned address. Before recording a
successful case, the runner checks GET=200 with the exact body, POST=405, and HEAD
with no body bytes. Salvo's GET route returns HEAD=405; axum supplies HEAD=200.
This behavioral difference is recorded rather than hidden by extra handlers.
The two root/path choices are compile-time features, not a shared binary with
runtime route selection. Build and smoke timeouts are configurable; server
processes are stopped on success and failure.

Cargo metadata from this workspace determines its normal target directory.
Results go into a fresh timestamp/PID directory below
`<target_directory>/http-server-size/`. The runner never cleans build caches,
reuses result JSON, or changes Cargo home, target, TEMP/TMP or environment variables.
It honors caller build configuration and uses Cargo's JSON executable path,
including configured target triples and custom artifact locations. The selected
target must produce executables runnable directly on the host; cross-target
runners are unsupported. No Docker build is involved.

Evidence includes commands, complete build logs, metadata, per-case HTTP results,
binary sizes and hashes, source/lockfile hashes, toolchain versions, Cargo config
file hashes and relevant build environment values. Configured target flags and
profile overrides can change the experiment, so compare only compatible recorded
environments. Configuration contents are not copied. The executable byte count
excludes PDBs, system shared libraries, linker maps, compression and packaging.
Each measured executable is copied into its case directory before the smoke
check, preserving its bytes when Cargo rebuilds the next route variant.

This differs from `../feature-costs`, whose binary only constructs a Router and
measures feature build costs. It also differs from `../runtime-costs`, whose
executable includes a client, assertions and instrumentation. No size ordering or
performance conclusion is implied without a new measured result and its context.

## Lock maintenance

When the root workspace's local Salvo packages change version, update this
fixture's lock from `benchmarks/http-server-size`:

```sh
cargo update --offline -p salvo -p salvo_core -p salvo_macros -p salvo-serde-util
cargo metadata --offline --locked --format-version 1
```

Commit the local package version changes alongside the workspace release.
Check the lock diff to ensure external package entries did not change; dependency
upgrades are a separate experiment. The metadata check deliberately resolves
dependencies (no `--no-deps`) to validate the refreshed lock.
