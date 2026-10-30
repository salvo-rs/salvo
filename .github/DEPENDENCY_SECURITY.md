# Dependency security checks

The root, examples, and fuzz workspaces intentionally ignore Cargo.lock. Security
CI generates each lockfile before auditing it, and runs weekly as well as on
dependency changes. Existing local lockfiles should be refreshed with
`cargo update` before interpreting findings as the current dependency resolution.

The direct rustls and quinn minimums exclude known vulnerable releases of those
packages. The core TLS feature activates rustls as a normal optional dependency,
so its minimum also applies to downstream server builds without workspace dev
dependencies. CI checks that an isolated downstream TLS application cannot resolve
rustls 0.23.40. Transitive dependency findings remain covered by the audit job; raising
a direct dependency minimum does not guarantee every transitive version is safe.

## RUSTSEC-2026-0387: RustCrypto JWT dependency blocker

As checked on 2026-10-30, the latest published `jsonwebtoken` is 11.1.0. Its
`rust_crypto` feature requires `p256 ^0.13.2` and `p384 ^0.13`, which require
`elliptic-curve 0.13` and `crypto-bigint 0.5.5`. The patched crypto-bigint series
starts at 0.7.6 and cannot satisfy that dependency chain. Upstream tracks the
blocked migration in [jsonwebtoken issue 549](https://github.com/Keats/jsonwebtoken/issues/549),
closed as not planned while the RustCrypto stack awaits rsa 0.10. Stable
`p256 0.14.0`, `p384 0.14.0`, and `crypto-bigint 0.7.10` are now published and
declare Rust versions below 1.95, but they do not satisfy jsonwebtoken's current
requirements. Raising Salvo's Rust version or updating the lockfile cannot bridge
these incompatible dependency ranges. Published requirements can be checked in
the [jsonwebtoken registry entry](https://index.crates.io/js/on/jsonwebtoken).

Salvo enables this JWT backend through `salvo-jwt-auth/ring` or the umbrella
`salvo/jwt-auth-ring` feature. The separately named `salvo/ring` feature selects
the TLS backend and does not, by itself, select the RustCrypto JWT backend.
Cargo features are additive: selecting AWS-LC at runtime does not remove a
RustCrypto dependency that another enabled feature pulled into the lockfile.

There is **no audit exception** for RUSTSEC-2026-0387. The root workspace audit
and the release workflow's audit before any crate is published remain blocked
until a supported upstream release removes the affected chain. Replacing the
lockfile or installing a different runtime
provider is not a resolution for a lockfile that still contains it. Do not
resume publication on the strength of the default-only consumer check below.

An isolated consumer using only the default JWT AWS-LC backend does not require
the affected RustCrypto chain; CI audits that exact consumer separately to
guard this feature boundary. Applications that can use that backend should
disable the alternative JWT backend and audit their own final lockfile. This
is a narrower dependency configuration, not a fix for the RustCrypto backend.
The advisory concerns secret integer operations; dependency presence alone
does not establish a timing attack against Salvo's token verification path.

After an upstream fix, update the dependency requirement, audit all three
workspace lockfiles and isolated JWT consumers, and remove this blocker only
after the RustCrypto feature also passes. See the
[RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2026-0387.html).

## RUSTSEC-2023-0071: rsa timing side channel

- Owner: Salvo maintainers. Review by **2027-01-23**, and whenever certon,
  jsonwebtoken, or their cryptographic call sites change.
- Applies to rsa 0.9 in certon 0.3 (salvo-acme) and in jsonwebtoken 11's optional
  RustCrypto backend (salvo-jwt-auth's `ring` feature). There is no
  patched rsa version listed in the advisory at the time of this review.
- certon's crypto module uses rsa for RSA key generation, PKCS#1/PKCS#8 parsing,
  and DER encoding. It does not use rsa private-key decryption. TLS signing and
  handshakes use rustls with ring/aws-lc instead. Salvo defaults new ACME keys to
  ECDSA P-256; applications may explicitly select RSA keys.
- Salvo's JWT middleware and OIDC decoder only verify signatures with public RSA
  keys; neither performs private-key decryption. Application code using
  jsonwebtoken directly must be reviewed separately.
- This is an audit exception for these reviewed call paths, not a claim that rsa
  is fixed or suitable for private-key decryption. Remove the exception when a
  fixed version or replacement is available. Reassess it before adding any new
  use of rsa, including through other dependencies.

Security CI fails on vulnerability advisories and `unsound` informational
advisories, except for explicitly reviewed entries in `.cargo/audit.toml`.
Unmaintained and yanked packages remain visible warnings for maintenance review.
The workflow tests this policy with an offline advisory fixture and checks the
exception review date so that it cannot remain unreviewed indefinitely.

As checked on 2026-10-30, the latest published `rbdc` is still 4.9.10. The
examples workspace resolves `rbdc 4.9.10 -> lru 0.14.0`, which is
affected by RUSTSEC-2026-0002 and RUSTSEC-2026-0253. The first fixed `lru` releases
for these advisories are 0.16.3 and 0.18.2 respectively; rbdc's `^0.14.0`
requirement cannot select them. The examples audit must remain blocked until the
upstream dependency is upgraded or replaced. No exception is added for this path.
The dependency finding alone does not establish that the example reaches the
affected iterator or panic-unwinding operations.

The current [rbdc registry entry](https://index.crates.io/rb/dc/rbdc) still
requires `lru ^0.14.0`. Fixed lru releases support Rust 1.95, but cannot satisfy
that requirement. A supported rbdc release must update it before a compatible
dependency refresh can clear this blocker.

Fresh lockfiles generated with Rust 1.95 on 2026-10-30 were audited against
RustSec database commit `ee1a49a2769a24a596afc1c20cb33748d2769586` with
`cargo audit --deny unsound`: the root and examples workspaces still fail on
the blockers above, and fuzz passes. The isolated default JWT AWS-LC consumer
also passes; this does not clear the root workspace blocker.

The unused direct `dotenv` dependency was removed from the OIDC Clerk example;
it did not load a `.env` file. No remaining example directly depends on the
unmaintained `dotenv` or `rustls-pemfile` packages. `rustls-pemfile 2.2.0` remains
an unmaintained warning through `rbdc 4.9.10` and
`toasty-driver-postgresql 0.8.0`; addressing those paths requires upstream
dependency changes or a database stack migration.
