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
- `cache_control`: repeated fields equal their comma-joined form, directive durations remain bounded,
  and opaque quoted extension content cannot override real cache directives.
- `openapi_roundtrip`: arbitrary accepted documents serialize/deserialize to the same canonical JSON;
  generated documents preserve references, schema examples, media examples, descriptions, and extensions.
- `conditional_headers`: model-checks ETag/date precedence across GET/HEAD and write methods,
  success/error statuses, weak tags, wildcards, and forged request ETags.
- `multipart`: generated parts must preserve exact fields/file bytes within count limits and reject
  overflow; arbitrary malformed bodies can never exceed accepted part/file limits.
- `tus_state`: bounded POST/PATCH/HEAD/DELETE sequences run against a temporary disk store.
  Good writes advance the modeled offset and exact disk bytes; offset conflicts, oversized writes,
  and wrong media types preserve state; completion and deletion are checked on every input.

The five property targets call the same functions as ordinary `cargo test --lib` tests.
They limit input to 8 KiB, multipart to eight parts, uploads to 32 bytes/16 mutations,
async operations to ten seconds, and use a single reusable Tokio runtime. All HTTP requests
use Salvo's in-process TestClient; they do not access external services.

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


## Repeatable regression and CI

Run the checked-in corpus and deterministic models without libFuzzer or sanitizers (also on Windows):

```bash
cd fuzz
cargo test --lib
cargo check --bins
```

The deterministic tests cover all 256 conditional-header flags and 256 bounded multipart/upload
models in addition to checked-in seed files. A panic names the replayed file. These tests check
semantics rather than only asserting that parsing does not panic.

Pull requests touching covered code run every target for 30 seconds on Linux/nightly with
AddressSanitizer. Weekly and manually dispatched runs use 300 seconds. Each uses libFuzzer seed
1337 and a maximum input length of 8192. A fixed seed assists reproduction; corpus evolution,
compiler versions, and execution speed can still change the exact exploration sequence.

The workflow restores `.corpus-cache/<target>`, merges the checked-in seeds into it, and saves
the evolved corpus after trusted scheduled/manual runs, including failures. Pull requests may
read the base-branch corpus but never publish reusable corpus caches. Crashes, timeout inputs,
working corpus, seeds, and logs are uploaded with `if: always()` for 14 days. `pipefail` retains
cargo-fuzz's failing status while `tee` records its diagnostics. Download the target's artifact
from the failed GitHub Actions run before it expires.

Replay a downloaded failure exactly, then minimize it on Linux/nightly:

```bash
cd fuzz
cargo +nightly fuzz run tus_state /absolute/path/to/crash-input -- -runs=1
cargo +nightly fuzz tmin tus_state /absolute/path/to/crash-input
```

Copy a minimized reproducer to `corpus/<target>/` and commit it. For the five property targets,
`cargo test --lib checked_in_corpus_replays_the_same_properties -- --nocapture` then replays it
without sanitizer tooling. The original libFuzzer input and full log remain the source for
reproduction; never replace them with a screenshot or only a panic message.
