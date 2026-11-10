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

The fresh root lockfile still contains `jsonwebtoken 11.1.0 -> p256/p384 0.13 ->
elliptic-curve 0.13.8 -> crypto-bigint 0.5.5`. The patched crypto-bigint range is
`>=0.7.6`, which does not satisfy this upstream requirement. Salvo enables this
JWT backend through `salvo-jwt-auth/ring` or `salvo/jwt-auth-ring`; the separate
`salvo/ring` feature selects TLS crypto only. Cargo features are additive, so
choosing AWS-LC at runtime does not remove an enabled RustCrypto dependency.

Owner: Salvo maintainers. Recheck on upstream jsonwebtoken/RustCrypto releases
and before publishing a dependency refresh. Upgrade a supported upstream graph,
then audit all affected feature configurations. There is no exception for this
finding. It concerns secret-integer operations; its presence does not by itself
prove a timing attack on public-key token verification.
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

The new snapshot includes `sqlx 0.8.6 -> sqlx-sqlite 0.8.6 -> libsqlite3-sys
0.30.1` through the SQLx and SeaORM examples. RUSTSEC-2026-0459 covers bundled
SQLite before 3.54.0; libsqlite3-sys `>=0.39.0` contains the patched bundled
SQLite. Its attack requires opening a less-trusted database and sibling WAL
file with mismatched page sizes. System SQLite and SQLCipher have their own
version/exposure conditions. A lockfile entry alone does not establish that an
example enables the affected bundle or opens attacker-controlled files.
[Advisory](https://rustsec.org/advisories/RUSTSEC-2026-0459.html).

The resolution also retains `rbdc 4.9.10 -> lru 0.14.0`. Its requirement cannot
select the fixed lru releases for RUSTSEC-2026-0002 (`>=0.16.3`) and
RUSTSEC-2026-0253 (`>=0.18.2`). rustls-pemfile 2.2.0 remains an unmaintained
warning through rbdc and toasty-driver-postgresql. Owner: Salvo maintainers;
recheck on database-stack upgrades and before dependency refreshes. Supported
upstream migrations are required; no exception has been added for these paths.
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
