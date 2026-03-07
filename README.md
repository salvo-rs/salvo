<div align="center">
<p><img alt="Salvo" width="132" style="max-width:40%;min-width:60px;" src="https://salvo.rs/images/logo-text.svg" /></p>

<h3>A powerful and simple Rust web framework</h3>

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
</p>
</div>

## Features

- **Simple & Powerful** - Minimal boilerplate. If you can write a function, you can write a handler.
- **HTTP/1, HTTP/2 & HTTP/3** - Full protocol support out of the box.
- **Flexible Routing** - Tree-based routing with middleware support at any level.
- **Auto TLS** - ACME integration for automatic certificate management.
- **OpenAPI** - First-class OpenAPI support with auto-generated documentation.
- **WebSocket & WebTransport** - Real-time communication built-in.
- **Built on Hyper & Tokio** - Production-ready async foundation.

## Quick Start

Create a new project:
```bash
cargo new hello-salvo
cd hello-salvo
cargo add salvo tokio --features salvo/oapi,tokio/macros
```

Write your first app in `src/main.rs`:
```rust
use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    let router = Router::new().get(hello);
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await;
    Server::new(acceptor).serve(router).await;
}
```

Run it:
```bash
cargo run
```

## Why Salvo?

### Middleware = Handler

No complex traits or generics. Middleware is just a handler:

```rust
#[handler]
async fn add_header(res: &mut Response) {
    res.headers_mut().insert(header::SERVER, HeaderValue::from_static("Salvo"));
}

Router::new().hoop(add_header).get(hello)
```

### Tree Routing with Middleware

Apply middleware to specific route branches:

```rust
Router::new()
    // Public routes
    .push(Router::with_path("articles").get(list_articles))
    // Protected routes
    .push(Router::with_path("articles").hoop(auth_check).post(create_article).delete(delete_article))
```

### OpenAPI in One Line

Just change `#[handler]` to `#[endpoint]`:

```rust
#[endpoint]
async fn hello() -> &'static str {
    "Hello World"
}
```

### Auto HTTPS with ACME

Get TLS certificates automatically from Let's Encrypt:

```rust
let listener = TcpListener::new("0.0.0.0:443")
    .acme()
    .add_domain("example.com")
    .http01_challenge(&mut router)
    .quinn("0.0.0.0:443"); // HTTP/3 support
```

## CLI Tool

```bash
cargo install salvo-cli
salvo new my_project
```

## Learn More

- [Official Website](https://salvo.rs)
- [API Documentation](https://docs.rs/salvo)
- [Examples](./examples/)

## Performance

Salvo consistently ranks among the fastest Rust web frameworks:
- [Web Frameworks Benchmark](https://web-frameworks-benchmark.netlify.app/result?l=rust)
- [TechEmpower Benchmarks](https://www.techempower.com/benchmarks/#section=data-r23)

## Support

If you find Salvo useful, consider [buying me a coffee](https://ko-fi.com/chrislearn).

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT).
