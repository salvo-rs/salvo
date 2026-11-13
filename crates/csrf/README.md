<div align="center">
<p><img alt="Salvo" width="132" style="max-width:40%;min-width:60px;" src="https://salvo.rs/images/logo-text.svg" /></p>
<p>
    <a href="https://github.com/salvo-rs/salvo/blob/main/README.md">English</a>&nbsp;&nbsp;
    <a href="https://github.com/salvo-rs/salvo/blob/main/README.zh.md">简体中文</a>&nbsp;&nbsp;
    <a href="https://github.com/salvo-rs/salvo/blob/main/README.zh-hant.md">繁體中文</a>
</p>
<p>
<a href="https://github.com/salvo-rs/salvo/actions">
    <img alt="build status" src="https://github.com/salvo-rs/salvo/workflows/ci-linux/badge.svg" />
</a>
<a href="https://github.com/salvo-rs/salvo/actions">
    <img alt="build status" src="https://github.com/salvo-rs/salvo/workflows/ci-macos/badge.svg" />
</a>
<a href="https://github.com/salvo-rs/salvo/actions">
    <img alt="build status" src="https://github.com/salvo-rs/salvo/workflows/ci-windows/badge.svg" />
</a>
<a href="https://codecov.io/gh/salvo-rs/salvo"><img alt="codecov" src="https://codecov.io/gh/salvo-rs/salvo/branch/main/graph/badge.svg" /></a>
<br>
<a href="https://crates.io/crates/salvo"><img alt="crates.io" src="https://img.shields.io/crates/v/salvo" /></a>
<a href="https://docs.rs/salvo"><img alt="Documentation" src="https://img.shields.io/docsrs/salvo.svg" /></a>
<a href="https://crates.io/crates/salvo"><img alt="Download" src="https://img.shields.io/crates/d/salvo.svg" /></a>
<a href="https://github.com/rust-secure-code/safety-dance/"><img alt="unsafe forbidden" src="https://img.shields.io/badge/unsafe-forbidden-success.svg" /></a>
<a href="https://blog.rust-lang.org/2026/04/16/Rust-1.95.0/"><img alt="Rust Version" src="https://img.shields.io/badge/rust-1.95%2B-blue" /></a>
<br>
<a href="https://salvo.rs">
    <img alt="Website" src="https://img.shields.io/badge/https-salvo.rs-%23f00" />
</a>
<a href="https://discord.gg/G8KfmS6ByH">
    <img src="https://img.shields.io/discord/1041442427006890014.svg?logo=discord">
</a>
<a href="https://gitcode.com/salvo-rs/salvo">
    <img src="https://gitcode.com/salvo-rs/salvo/star/badge.svg">
</a>
</p>
</div>

Salvo is an extremely simple and powerful Rust web backend framework. Only basic Rust knowledge is required to develop backend services.

# salvo-csrf

## CSRF (Cross-Site Request Forgery) middleware for Salvo.

This is an official crate, so you can enable it in `Cargo.toml` like this:

```toml
salvo = { version = "*", features = ["csrf"] }
```

[![Docs](https://img.shields.io/docsrs/salvo-csrf.svg)](https://docs.rs/salvo-csrf)

## Cipher scheduling

For high-throughput applications, prefer the keyed `HmacCipher`, `AesGcmCipher`,
or `CcpCipher` features. Bcrypt intentionally performs substantially more CPU work.
The middleware and built-in stores use asynchronous cipher operations; bcrypt
runs on blocking workers with a process-wide limit equal to available CPU parallelism.
Cancelling a request aborts queued work; already-running work retains its capacity
permit until it finishes. Apply request timeouts and admission limits to bound
requests waiting for capacity. Direct `generate()` and `verify()` calls remain
synchronous; custom expensive ciphers can override `generate_async()` and
`verify_async()` while existing cipher implementations keep working.

## Choosing token storage

For browser-facing HTTPS services, prefer `CookieStore::host_prefixed()` over
the compatibility default `CookieStore::new()`:

```rust,ignore
use salvo::csrf::{BcryptCipher, CookieStore, Csrf, FormFinder};
let csrf = Csrf::new(
    BcryptCipher::new(),
    CookieStore::host_prefixed(),
    FormFinder::new("csrf"),
);
```

The resulting `__Host-salvo.csrf` cookie has `Secure`, `HttpOnly`, `Path=/`,
and no `Domain`. Browsers reject attempts by sibling subdomains to set a
parent-domain cookie with that name. The server rejects invalid prefix
configurations, including later builder or public-field changes. The public
origin must use HTTPS; the constructor also works behind a trusted TLS-terminating
proxy because `Secure` is explicit rather than inferred from the internal hop.

The compatibility default is a double-submit cookie **without authentication
session binding**. `SameSite=Strict` does not separate untrusted sibling subdomains.
If an attacker can inject a known valid CSRF cookie and the application accepts
form submissions without another origin check, that pair can be replayed with
the victim's independent login cookie. Using a global HMAC key alone does not
bind the pair to that login session. A custom-header finder also depends on the
application's CORS and origin policy; do not grant credentialed access to untrusted
origins.

When the application already uses server-side sessions, enable `session-store`
and use `SessionStore`, with `SessionHandler` before `Csrf`. The proof then belongs
to that session; copying another session's token or CSRF cookie is insufficient.
Secure the authentication session cookie as well, and rotate/reissue session
state at the appropriate authentication boundary. Host-prefixed cookies prevent
cookie injection, but do not protect against same-origin script execution or
replace application authorization.

See the [OWASP CSRF prevention guidance](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html#signed-double-submit-cookie-recommended).

## ☕ Donate

Salvo is an open source project. If you want to support Salvo, you can ☕ [**buy me a coffee here**](https://ko-fi.com/chrislearn).

## ⚠️ License

Salvo is licensed under [Apache License, Version 2.0](LICENSE) ([http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0)).
