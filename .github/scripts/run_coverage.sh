#!/usr/bin/env bash
set -euo pipefail
mkdir -p coverage-artifacts
{
  git rev-parse HEAD
  rustc -Vv
  cargo -V
  cargo tarpaulin --version
} > coverage-artifacts/environment.txt 2>&1
cargo generate-lockfile 2>&1 | tee coverage-artifacts/resolve.log
cp Cargo.lock coverage-artifacts/Cargo.lock
cargo tarpaulin --locked --all-features --engine llvm --out xml \
  --output-dir coverage-artifacts 2>&1 | tee coverage-artifacts/tarpaulin.log
