# Contributing to Salvo

Thank you for contributing to Salvo.

## Before You Start

- Search existing issues and pull requests before starting new work.
- Open an issue first for behavior changes, large refactors, or new public APIs.
- Keep pull requests focused. Separate bug fixes, refactors, and documentation changes when possible.

## Development Setup

1. Install the Rust toolchain declared by the workspace `rust-version`.
2. Fork the repository and create a topic branch from `main`.
3. Clone your fork and enter the workspace root.

```bash
git clone https://github.com/<your-user>/salvo.git
cd salvo
```

## Project Layout

- `crates/` contains the Salvo workspace crates.
- `examples/` contains integration-style examples and sample apps.
- `.github/workflows/` reflects the checks that run in CI.

## Local Checks

Run these commands before opening a pull request:

```bash
cargo +nightly fmt --all -- --check
cargo check --all --bins --examples --tests
cargo test --all --all-features --no-fail-fast
cargo test --workspace --doc
```

Nightly-only checks used in CI:

```bash
cargo +nightly udeps
cargo +nightly hack check --feature-powerset --depth 1 -Z avoid-dev-deps --exclude-features server --at-least-one-of aws-lc-rs,ring --exclude-no-default-features
```

When your change affects examples, also verify the examples workspace:

```bash
cd examples
cargo check --all --bins --examples --tests
```

## Coding Guidelines

- Follow the existing crate naming, feature flag, and API style.
- Prefer additive changes over breaking public API changes.
- Add or update tests when behavior changes.
- Update README files, crate docs, or examples when user-facing behavior changes.
- Keep feature-gated code compile-tested for both enabled and disabled states.

## Documentation And Message Style

- Prefer compile-checked examples over `ignore` blocks. Use `no_run` when the snippet needs a real network listener or other runtime setup but should still compile.
- Keep examples aligned with the current API surface. If an example needs too much placeholder context, simplify it instead of leaving stale code in docs.
- Write custom error, log, `panic!`, and `expect!` messages in the same style used across the Rust ecosystem: sentence starts lowercase and normally has no trailing period.
- Prefer direct, grammatical wording that explains what is missing or why the operation failed.
- Do not mechanically rewrite standardized protocol text such as HTTP reason phrases, RFC terms, or other externally defined wire-format strings.

## Pull Requests

- Explain the motivation, scope, and user-visible impact.
- Link the relevant issue when one exists.
- Call out breaking changes, MSRV changes, or feature-flag changes explicitly.
- Include follow-up work separately instead of bundling unrelated cleanup.

## Release Notes

If your change should appear in release notes, add a short summary to the repository root `CHANGELOG.md` under `Unreleased`.

## Reporting Security Issues

Do not open a public issue for security vulnerabilities. Follow the process in `SECURITY.md`.
