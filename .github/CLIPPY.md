# Clippy policy

CI runs Clippy (rather than only installing the component) on the full workspace,
all targets and all features on Rust 1.99.1. The separate MSRV job remains responsible
for Rust 1.95 compatibility. Update the lint toolchain deliberately so new compiler
diagnostics can be reviewed together with the existing warning baseline.

The initial hard gate covers `await_holding_lock`, `if_let_mutex` and `dbg_macro`.
Other workspace lint levels stay unchanged: existing style/documentation warnings
are visible and retained as JSONL/stderr artifacts, rather than suppressed or
silently promoted to release blockers. Tighten individual correctness gates as
their baselines are reviewed; do not blanket-allow a lint to make a new warning pass.

The artifact includes the exact commit, compiler/Clippy version, resolved lockfile,
structured diagnostics and stderr. Local reproduction:

```sh
cargo clippy --workspace --all-targets --all-features -- \
  -D clippy::await_holding_lock -D clippy::if_let_mutex -D clippy::dbg_macro
```
