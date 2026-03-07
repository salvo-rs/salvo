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
<a href="https://blog.rust-lang.org/2025/08/07/Rust-1.89.0/"><img alt="Rust Version" src="https://img.shields.io/badge/rust-1.89%2B-blue" /></a>
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

# salvo-compression

Compression middleware for the Salvo web framework that automatically compresses HTTP responses, reducing bandwidth usage and improving load times.

## Features

- **Multiple algorithms**: Gzip, Brotli, Deflate, and Zstd compression
- **Content negotiation**: Automatically selects the best algorithm based on client's `Accept-Encoding` header
- **Configurable compression levels**: Choose between fastest, default, best compression, or precise control
- **Content type filtering**: Only compress appropriate MIME types (text, JSON, etc.)
- **Minimum length threshold**: Skip compression for small responses that wouldn't benefit

## Supported Algorithms

| Algorithm | Feature | Content-Encoding |
|-----------|---------|------------------|
| Gzip | `gzip` | `gzip` |
| Brotli | `brotli` | `br` |
| Deflate | `deflate` | `deflate` |
| Zstd | `zstd` | `zstd` |

## Installation

This is an official crate, so you can enable it in `Cargo.toml`:

```toml
salvo = { version = "*", features = ["compression"] }
```

## Quick Start

```rust
use salvo::prelude::*;
use salvo::compression::{Compression, CompressionLevel};

#[handler]
async fn hello() -> &'static str {
    "Hello World!"
}

#[tokio::main]
async fn main() {
    let compression = Compression::new()
        .enable_gzip(CompressionLevel::Default)
        .min_length(1024);  // Only compress responses > 1KB

    let router = Router::new()
        .hoop(compression)
        .get(hello);

    let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
    Server::new(acceptor).serve(router).await;
}
```

## Documentation & Resources

- [API Documentation](https://docs.rs/salvo-compression)
- [Example Projects](https://github.com/salvo-rs/salvo/tree/main/examples)

## ☕ Donate

Salvo is an open source project. If you want to support Salvo, you can ☕ [**buy me a coffee here**](https://ko-fi.com/chrislearn).

## ⚠️ License

Salvo is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0)).

- MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT)).
