# mTLS authentication and authorization

This example uses Rustls to require a certificate issued by a trusted client CA,
then authorizes one URI subject alternative name (SAN) in the HTTP handler. A
certificate from the same CA with a different identity receives HTTP 403.

## Run on Windows

Install OpenSSL 3, then run these PowerShell commands from the repository root.
The generation script creates a new directory and refuses to overwrite an
existing one. These unencrypted example keys are for local development only.

```powershell
./examples/mtls-rustls/generate-certs.ps1
$env:MTLS_CERT_DIR = 'D:/BuildCache/salvo-mtls-example/certs'
$env:CARGO_TARGET_DIR = 'D:/BuildCache/salvo-mtls-example/target'
$env:TEMP = 'D:/BuildCache/salvo-mtls-example/tmp'
$env:TMP = $env:TEMP
New-Item -ItemType Directory -Force $env:TEMP | Out-Null
cargo metadata --manifest-path examples/Cargo.toml --no-deps --format-version 1
cargo run --manifest-path examples/Cargo.toml -p example-mtls-rustls
```

In another PowerShell terminal:

```powershell
$certs = 'D:/BuildCache/salvo-mtls-example/certs'
# Trusted certificate with the authorized URI: HTTP 200.
curl.exe --cacert "$certs/ca.pem" --cert "$certs/reader.pem" --key "$certs/reader.key" https://localhost:8698/
# Trusted certificate with another URI: HTTP 403.
curl.exe --cacert "$certs/ca.pem" --cert "$certs/other.pem" --key "$certs/other.key" https://localhost:8698/
# No certificate: TLS failure, before any HTTP handler runs.
curl.exe --cacert "$certs/ca.pem" https://localhost:8698/
```

Use a curl build supporting PEM client certificates (for example curl built
with OpenSSL). Other TLS backends may require a different client key format.
An untrusted client certificate also fails at the TLS handshake. To allow
anonymous TLS connections, run the server with `--optional`; anonymous requests
then reach this handler and receive HTTP 401. Optional authentication still
rejects an invalid certificate when one is presented.

## Reading TLS information

`req.tls_info()` borrows connection metadata captured after the handshake. Its
`peer_certificates()` method returns the peer's DER chain, leaf first; an empty
chain means the TLS connection has no client certificate. `server_name()` and
`alpn_protocol()` expose the client's requested SNI name and the negotiated ALPN
protocol when present.
Plain HTTP and requests constructed without connection metadata return `None`.
See the `Request::tls_info` API documentation for supported listeners.

Certificate bytes are captured once per connection and shared across its
requests, including HTTP/2 streams. There is no certificate parsing or copying
in the per-request dispatch path. This example parses X.509 only when the handler
needs an application identity; core Salvo does not depend on an X.509 parser.

Rustls verifies the chain against the configured CA; the application still
decides which verified identities may access each resource. The example uses a
URI SAN allowlist. If your certificate policy uses a common name, it can be read
through `cert.subject().iter_common_name()`, but the identity mapping must match
your issuer's policy. A custom Rustls verifier defines its own trust guarantees;
the presence of certificate bytes alone does not replace certificate validation.

For TLS terminated at a reverse proxy, the backend connection describes the
proxy, not the original client. Forwarded certificate headers require a separate
trusted-proxy policy, including removal of client-supplied copies of those headers.
