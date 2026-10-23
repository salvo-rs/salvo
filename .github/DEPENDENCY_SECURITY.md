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

## RUSTSEC-2023-0071: rsa timing side channel

- Owner: Salvo maintainers. Review by **2027-01-23**, and whenever certon or its
  cryptographic call sites change.
- Applies to the rsa 0.9 dependency of certon 0.3, used by salvo-acme. There is no
  patched rsa version listed in the advisory at the time of this review.
- certon's crypto module uses rsa for RSA key generation, PKCS#1/PKCS#8 parsing,
  and DER encoding. It does not use rsa private-key decryption. TLS signing and
  handshakes use rustls with ring/aws-lc instead. Salvo defaults new ACME keys to
  ECDSA P-256; applications may explicitly select RSA keys.
- This is an audit exception for these reviewed call paths, not a claim that rsa
  is fixed or suitable for private-key decryption. Remove the exception when a
  fixed version or replacement is available. Reassess it before adding any new
  use of rsa, including through other dependencies.

All other advisories fail the audit. The workflow checks the exception review
date so that it cannot remain unreviewed indefinitely.
