# Dependency security checks

The root, examples, and fuzz workspaces intentionally ignore Cargo.lock. Security
CI resolves fresh lockfiles and audits weekly and on dependency changes. Refresh
old local lockfiles before interpreting their findings as a current resolution.

Audits remain **warning-only**: advisories and failures to complete the audit do
not fail CI or block releases. Dependency resolution and downstream TLS checks
still fail on errors. The wrapper now distinguishes `clean`, `findings`, and
`tool_error` in evidence metadata rather than treating a green job as a clean audit.
No advisory exception or publication policy is changed by this evidence refresh.

## Evidence and reproduction

Pass `--evidence-dir DIRECTORY` to `.github/scripts/audit_lockfile.py` to save the
raw JSON/stderr, actual lockfile, lockfile SHA-256, RustSec database commit,
command, original exit code, and a machine-readable outcome. Security CI uploads
these files even on failure for each workspace and the isolated default JWT
consumer. `tool_error` means no complete report was available, not zero findings.

The checked-in [2026-11-09 summary](security-audits/2026-11-09.json) records fresh
lockfiles audited with cargo-audit 0.22.0 and Rust 1.99.1, using RustSec DB
`899077d9d471b9fd3dbf4b2faea8ef26e83692e8` (1,429 advisories). The root/examples/fuzz
resolver honors workspace Rust 1.95; the isolated consumer used the invoking
Rust 1.99.1 toolchain. This is a dated snapshot, not a perpetual clean bill of health.

| Scope | Raw exit | Result |
|---|---|---|
| Root workspace | 1 | crypto-bigint 0.5.5 / RUSTSEC-2026-0387 |
| Examples | 1 | libsqlite3-sys 0.30.1 / RUSTSEC-2026-0459; lru 0.14.0 unsound advisories RUSTSEC-2026-0002 and RUSTSEC-2026-0253; rustls-pemfile unmaintained warning |
| Fuzz workspace | 0 | No findings in this resolution |
| Isolated default JWT AWS-LC consumer | 0 | No findings in this narrower resolution |

Reproduce a workspace audit after resolution with:

```sh
cargo generate-lockfile
python3 .github/scripts/audit_lockfile.py --file Cargo.lock --evidence-dir audit-evidence
```

For examples/fuzz pass their manifest to `cargo generate-lockfile` and their
lockfile to the wrapper. To test the isolated JWT consumer, create a separate
workspace containing only a default `salvo-jwt-auth` path dependency. Do not
substitute the narrower consumer for the root or RustCrypto feature graph.

## RustCrypto JWT: RUSTSEC-2026-0387

The workspace now requires jsonwebtoken 12. Its RustCrypto backend uses
`p256/p384 0.14 -> elliptic-curve 0.14 -> crypto-bigint 0.7`, and RSA uses the
0.10 family. A fresh root resolution on 2026-11-15 selected crypto-bigint
0.7.11 and rsa 0.10.0 for JWT, removing the crypto-bigint 0.5 path reported in
the dated 2026-11-09 snapshot above. cargo-audit against RustSec DB
`02e2517f457a359fc6d3440ea4560c8ea188fee8` reported no findings under the existing
RSA exception below. No new advisory exception was added.

The patched crypto-bigint range is `>=0.7.6`. Refresh and audit application
lockfiles rather than assuming a compatible version range guarantees a patched
resolution. Salvo's `salvo-jwt-auth/ring` and `salvo/jwt-auth-ring` features select
the RustCrypto JWT backend; the separate `salvo/ring` feature selects TLS crypto.
Cargo features are additive, so choosing AWS-LC at runtime does not remove an
enabled RustCrypto dependency. Both JWT providers and their combined feature
configuration are tested.

`ConstDecoder::from_secret`, `DecodingKey::from_secret`, and
`EncodingKey::from_secret` now return `Result` and reject empty HMAC secrets.
Handle constructor errors during application configuration when upgrading.
[Advisory](https://rustsec.org/advisories/RUSTSEC-2026-0387.html).

## RSA exception: RUSTSEC-2023-0071

Owner: Salvo maintainers. Review by **2027-01-23**, or whenever certon,
jsonwebtoken, or cryptographic call sites change. The current path is
`salvo-acme -> certon 0.4.0 -> rsa 0.9.10`, not certon 0.3. In certon 0.4,
`crypto.rs` uses RSA for key generation, PKCS#1/PKCS#8 parsing and DER encoding;
it does not call private-key decryption. ACME JWS uses ECDSA P-256, and TLS
signing/handshakes use rustls providers. Salvo defaults new ACME keys to ECDSA.
The optional JWT RustCrypto backend verifies RSA signatures with public keys.

The current advisory lists rsa `>=0.10.0` as patched; certon 0.4 still requires
the 0.9 series. This existing exception covers these reviewed call paths only;
it does not say the resolved RSA version is fixed or suitable for private-key
decryption. Review application code using jsonwebtoken/rsa directly separately,
and remove the exception after a supported upstream migration or replacement.
[Advisory](https://rustsec.org/advisories/RUSTSEC-2023-0071.html).

## Examples: database dependency findings

The SQLx examples now use SQLx 0.9, and both SeaORM examples and their
migration crate use stable SeaORM 2.0.4. SQLx 0.9's published SQLite driver still
caps `libsqlite3-sys` below 0.38, so the examples workspace temporarily patches
`sqlx-sqlite` 0.9.0 with an otherwise unchanged copy requiring libsqlite3-sys
0.39. Its [patch notes](../examples/vendor/sqlx-sqlite/LOCAL_PATCH.md) record the
source, licenses, exact change, and removal conditions.

A fresh examples resolution on 2026-11-15 selected libsqlite3-sys 0.39.1.
The SQLite in-memory smoke test checks that the linked engine is at least
3.54.0, runs the example's migrations, and exercises insert/update/read/delete.
All SQLx/SeaORM example targets compile. The dated
[database refresh evidence](security-audits/2026-11-15-sqlite.json) records the
remaining RBatis/Toasty warnings separately; they are not exceptions or a claim
that the entire examples graph is clean.

RUSTSEC-2026-0459 concerns bundled SQLite before 3.54.0. Its attack requires
opening a less-trusted database and sibling WAL file with mismatched page sizes.
System SQLite and SQLCipher have their own version/exposure conditions.
[Advisory](https://rustsec.org/advisories/RUSTSEC-2026-0459.html).

The RBatis example now explicitly patches rbdc 4.9.11. Upstream 4.9.11 and
its HEAD still require lru 0.14 and rustls-pemfile 2; the local compatibility
patch requires lru >=0.18.2 and parses PEM through maintained rustls-pki-types.
This removes its paths to RUSTSEC-2026-0002, RUSTSEC-2026-0253 and unmaintained
RUSTSEC-2025-0134. Explicit empty or invalid custom CA input is rejected before
a handshake. [Patch notes](../examples/vendor/rbdc/LOCAL_PATCH.md) preserve the
published source commit, version, license, exact diff, and removal conditions.
Upstream statement-cache tests and local TLS handshakes pass, including invalid
roots and missing/malformed key fixtures; the RBatis example compiles.

The RBatis refresh removes lru 0.14. Its dated
[RBatis evidence](security-audits/2026-11-15-rbdc.json) was generated on the
independent branch and records the then-remaining Toasty PEM path.

The Toasty PostgreSQL driver is now explicitly patched
in the examples workspace to use maintained rustls-pki-types PEM APIs; its
published 0.8 version is retained because latest 0.11 also uses the old parser.
The [patch notes](../examples/vendor/toasty-driver-postgresql/LOCAL_PATCH.md)
record source, full MIT license, exact diff, tests and removal conditions.

Sixteen isolated driver tests pass, including valid client identities and
rejection of empty/malformed certificates, missing/invalid keys and mismatched
keys. The Toasty example compiles. The dated
[Toasty evidence](security-audits/2026-11-15-toasty.json) records the
then-remaining rbdc PEM path in its independent snapshot. No advisory
exception has been added. Owner: Salvo maintainers; recheck when a supported
upstream driver migrates away from the unmaintained parser.
Actual iterator/panic-unwind reachability must be assessed in each application.


The direct rustls/quinn minimums and isolated downstream TLS checks remain in
place. They exclude known affected direct releases, but do not prove every
transitive dependency or feature combination safe. The offline audit-policy
fixture verifies unsound advisories remain visible without changing warning-only
policy; the evidence tests distinguish findings from audit execution errors.

Warnings without a RustSec advisory (for example a yanked package) are retained in
artifact metadata as a package diagnostic with its warning kind. Advisory-backed
warnings retain their advisory IDs. Both are `findings` under the same warning-only
policy; a null advisory does not indicate an unfinished audit.
