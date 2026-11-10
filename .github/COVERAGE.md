# Coverage diagnostics

The coverage workflow pins Rust 1.99.1 and cargo-tarpaulin 0.37.6 with its LLVM
engine. This is the combination from the successful main run
[42640287317](https://github.com/salvo-rs/salvo/actions/runs/42640287317).
Update the pair deliberately and verify the full Linux run before adopting it.

`run_coverage.sh` preserves the commit, compiler/tool versions, resolved lockfile,
resolution log and tarpaulin output in `coverage-artifacts`, even when coverage
fails. CI uploads that directory using `always()`; the wrapper uses `pipefail`
so tee cannot hide a failed measurement. Reports are uploaded to Codecov only
after successful coverage. Reproduce with `bash .github/scripts/run_coverage.sh`.

Run `python3 .github/scripts/test_run_coverage.py` to verify the failure path
without running the full instrumented workspace. The fixture intentionally
returns status 7 and checks that diagnostics survive.
