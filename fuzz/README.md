# Salvo fuzzing

This directory contains the standalone `cargo-fuzz` workspace for parser- and header-heavy
surfaces in Salvo.

## Prerequisites

```bash
cargo install cargo-fuzz --locked
```

`cargo-fuzz` requires a nightly toolchain when running targets.

## Targets

- `path_filter`: fuzzes `PathFilter::new` and `PathFilter::try_new` with malformed route patterns.
- `basic_auth`: fuzzes Basic auth header parsing with arbitrary encoded and malformed credentials.
- `tus_options`: fuzzes TUS upload ID extraction and upload URL generation from request headers.
- `websocket_upgrade`: fuzzes WebSocket handshake validation and subprotocol negotiation.

## Usage

Run a target:

```bash
cd fuzz
cargo +nightly fuzz run path_filter
```

Run with a fixed time budget:

```bash
cd fuzz
cargo +nightly fuzz run websocket_upgrade -- -max_total_time=300
```

## Corpus

Each target has a small seed corpus in `fuzz/corpus/<target>/`. Add reduced crash reproducers there
when fuzzing finds a new issue that should remain covered.
