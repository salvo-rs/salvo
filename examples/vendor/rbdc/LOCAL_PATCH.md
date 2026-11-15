# Temporary RBatis driver dependency patch

This is the published `rbdc` 4.9.11 crate from crates.io, upstream
`rbatis/rbdc` commit `2202e09b98156d66406531c73efab76d7ecfbf14`.
The original version, Apache-2.0 LICENSE, and upstream tests are preserved.
Both the latest release and upstream HEAD `2da5e08b605094116d2455899be939a982f843bb`
still used the affected dependencies when checked on 2026-11-15.

Local changes:

- Require lru >=0.18.2 instead of 0.14, fixing RUSTSEC-2026-0002 and
  RUSTSEC-2026-0253. Statement-cache behavior is unchanged and tested.
- Remove unmaintained rustls-pemfile (RUSTSEC-2025-0134); use maintained
  rustls-pki-types PEM parsing through rustls' re-export for CA certificates
  and TLS test fixtures. Explicitly supplied empty or non-certificate roots
  now return an error before a handshake. TLS verification remains enabled.
- Extend local TLS tests for invalid/empty roots and missing/malformed keys.
  Tests explicitly install a TLS provider because the examples feature graph
  also enables AWS-LC; production provider selection is unchanged.

The examples workspace applies this compatibility patch explicitly. No Cargo
cache or registry source is modified. Remove this directory and the workspace
patch when a supported upstream release contains both migrations. Refresh the
lockfile, run upstream statement-cache/TLS tests, compile the RBatis example,
and audit the examples graph before removing or refreshing it. Preserve any
other database patches and investigate each remaining advisory separately.
