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

## RUSTSEC-2026-0387: crypto-bigint timing side channels

- Owner: Salvo maintainers. Reviewed **2026-10-27**; review again by
  **2027-01-23**, and whenever jsonwebtoken or the JWT call sites change.
- Dependency path: salvo-jwt-auth's optional `ring` feature enables
  `jsonwebtoken/rust_crypto`, whose p256 0.13 and p384 0.13 dependencies use
  elliptic-curve 0.13 and crypto-bigint 0.5.5. Cargo includes these optional
  dependencies in the root lockfile even when only AWS-LC is enabled.
- The [advisory](https://rustsec.org/advisories/RUSTSEC-2026-0387.html) is fixed
  in crypto-bigint >=0.7.6. jsonwebtoken 11.1.0 still requires p256/p384 0.13,
  so a lockfile update cannot select the fixed crypto-bigint for this path.
- JwtAuth, ConstDecoder, and OidcDecoder use this backend for signature
  verification with public EC keys and public signature values. They do not
  perform EC private-key generation or signing. HMAC uses separate primitives.
  certon's EC dependencies resolve crypto-bigint >=0.7.6.
- This exception covers those verification call paths only. Applications doing
  private-key signing through jsonwebtoken must use the default AWS-LC provider
  or assess their exposure separately. Enabling only `jwt-auth-ring` selects
  RustCrypto for JWTs, despite the feature name; it does not use ring for JWTs.
- Remove this exception when jsonwebtoken supports fixed dependencies or a
  replacement backend is adopted. It does not mean crypto-bigint 0.5 is fixed.

All other advisories fail the audit. The workflow checks the exception review
date so that these exceptions cannot remain unreviewed indefinitely.
