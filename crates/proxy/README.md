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
<a href="https://docs.rs/salvo"><img alt="Documentation" src="https://docs.rs/salvo/badge.svg" /></a>
<a href="https://crates.io/crates/salvo"><img alt="Download" src="https://img.shields.io/crates/d/salvo.svg" /></a>
<a href="https://github.com/rust-secure-code/safety-dance/"><img alt="unsafe forbidden" src="https://img.shields.io/badge/unsafe-forbidden-success.svg" /></a>
<a href="https://blog.rust-lang.org/2025/12/11/Rust-1.92.0/"><img alt="Rust Version" src="https://img.shields.io/badge/rust-1.92%2B-blue" /></a>
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

# salvo-proxy

## Proxy middleware for Salvo

This crate provides proxy capabilities for the Salvo web framework, allowing you to forward requests to upstream servers. It's useful for creating API gateways, load balancers, and reverse proxies.

### Features

- Support for HTTP and HTTPS proxying
- Multiple upstream server selection strategies
- WebSocket connection support
- Header manipulation
- Path and query rewriting
- Multiple HTTP client backends (Hyper, Reqwest)

### Usage

This is an official crate, so you can enable it in `Cargo.toml` like this:

```toml
salvo = { version = "*", features = ["proxy"] }
```

### Path forwarding

The default path getter forwards the wildcard tail captured by routes such as
`Router::with_path("api/{**rest}")`. If the upstream is `http://backend`, the
gateway path `/api/users` is forwarded to `http://backend/users`. If the backend
also expects the `/api` prefix, configure the upstream with that base path:
`http://backend/api`.

`Proxy` normalizes literal `.` and `..` path segments before forwarding. When
the proxy is used as an access-control boundary and the upstream may perform an
additional URL decode pass, enable strict path normalization to reject ambiguous
encoded path characters:

```rust
use salvo::prelude::*;
use salvo::proxy::Proxy;

let router = Router::with_path("api/{**rest}").goal(
    Proxy::use_hyper_client("http://backend/api")
        .strict_path_normalization(true),
);
```

Strict path normalization rejects percent-encoded `.`, `/`, `\`, and `%`
characters in the proxied path tail, including cases such as `%2e%2e`,
`%2f`, `%5c`, and double-encoded forms that remain percent-encoded after
Salvo routing extracts the tail. Backends should still validate and normalize
their own routes and file paths; this option only prevents common proxy/backend
path interpretation mismatches.

[![Docs](https://docs.rs/salvo-proxy/badge.svg)](https://docs.rs/salvo-proxy)

## ☕ Donate

Salvo is an open source project. If you want to support Salvo, you can ☕ [**buy me a coffee here**](https://ko-fi.com/chrislearn).

## ⚠️ License

Salvo is licensed under [Apache License, Version 2.0](LICENSE) ([http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0)).
