# Temporary Toasty PostgreSQL PEM compatibility patch

This is the published `toasty-driver-postgresql` 0.8.0 source from crates.io,
upstream `tokio-rs/toasty` commit
`4b00415a2c9f66114d548d0fc1a466f867b5dc31`, path
`crates/toasty-driver-postgresql`. Its original version is preserved. The full
MIT LICENSE is copied from that same upstream commit because the crate archive
does not contain it. The latest 0.11.0 driver was checked on 2026-11-15 and
still depends on rustls-pemfile; staying on 0.8 avoids an unrelated ORM API
migration while fixing the existing example.

Local changes remove unmaintained rustls-pemfile (RUSTSEC-2025-0134) and use
maintained rustls-pki-types PEM APIs through rustls' re-export. Root and client
certificate iteration remains fallible, empty certificate input is rejected,
and an absent private key is an error. TLS verification and client identity
validation remain enabled. `UPSTREAM.patch` records the complete manifest and
source diff. The added test-only dependencies generate throwaway identities and
normal temporary fixture files; no database or external service is required.
The package has an isolated workspace for these test-only dependencies. Run
`cargo test --manifest-path examples/vendor/toasty-driver-postgresql/Cargo.toml --lib`
from the repository root; Cargo uses that package's normal target directory.

Tests cover valid CA/client identities, empty/malformed/non-certificate roots,
missing identity files, invalid PEM/DER, absent private keys, mismatched keys,
and certificate/key option pairing. The PostgreSQL Toasty example is compiled
separately. The examples workspace applies this patch explicitly; registry and
Cargo cache sources are not modified.

Remove this directory and its workspace patch when a supported Toasty driver
release migrates to a maintained PEM parser. Refresh the lockfile, run these TLS
configuration regressions, compile the example, and audit the complete examples
graph when removing or refreshing the patch. Preserve independent patches for
other database dependencies and do not silence their findings here.
