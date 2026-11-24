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
</p>
</div>

## Differences from Upstream

This repository is a fork of [salvo-rs/salvo](https://github.com/salvo-rs/salvo) maintained on the `feat/serverless` branch. It stays compatible with the upstream public API while adding the following:

### 1. Serverless Runtimes (AWS Lambda + Cloudflare Workers)

- A dedicated `salvo-serverless` crate adapts a single `Router` to **two** runtimes — **AWS Lambda** and **Cloudflare Workers** — with zero code changes between them.
- The `#[service]` attribute macro turns a zero-argument router builder into the platform entry point (a Lambda handler or a Workers `#[worker::event]`), with optional one-time `start = <path>` warm-start initialization.
- Opt-in `rt_lambda` and `rt_worker` features gate each runtime; the Workers adapter compiles to `wasm32-unknown-unknown`.
- Request and response bodies stream end-to-end on both runtimes. The Workers adapter exposes Cloudflare bindings (D1, R2, Queue, KV) and a WebSocket compatibility layer that lets existing `WebSocketUpgrade` handlers run unchanged on `wasm32`.

### 2. Zero-Copy Request Pipeline

- Bodies are cached as reference-counted `bytes::Bytes` instead of allocating `Vec<u8>` per request.
- Path and query deserialization borrows from the URI via `Cow<'_, str>`.
- Static files and HTTP `Range` responses serve borrowed slices directly.
- Short query keys/values are stored inline in `CompactString`.

### 3. Engineering & CI

- A dedicated Serverless CI workflow cross-compiles the Workers example to `wasm32-unknown-unknown` on every push.
- An `upstream-watch` workflow automatically opens an issue when upstream `main` advances, making rebases easy to track.
- Expanded test, benchmark and security-audit suites.

## Features

- **Simple & Powerful** - Minimal boilerplate. If you can write a function, you can write a handler.
- **HTTP/1, HTTP/2 & HTTP/3** - Full protocol support out of the box.
- **Flexible Routing** - Tree-based routing with middleware support at any level.
- **Auto TLS** - ACME integration for automatic certificate management.
- **OpenAPI** - First-class OpenAPI support with auto-generated documentation.
- **WebSocket & WebTransport** - Real-time communication built-in.
- **Zero-Copy Design** - Request parsing, body handling and static files borrow `Bytes`/`Cow` to avoid redundant allocations.
- **Serverless Ready** - Deploy the same `Router` to AWS Lambda and Cloudflare Workers with zero code changes.
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

### JSON APIs

Salvo handlers can deserialize request bodies and return typed JSON responses:

```rust
use salvo::http::ParseError;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct CreateTodo {
    text: String,
}

#[derive(Serialize)]
struct Todo {
    id: u64,
    text: String,
}

#[handler]
async fn create_todo(req: &mut Request) -> Result<Json<Todo>, ParseError> {
    let payload = req.parse_json::<CreateTodo>().await?;
    Ok(Json(Todo {
        id: 1,
        text: payload.text,
    }))
}
```

For this pattern, add Serde's derive feature:

```bash
cargo add serde --features derive
```

### Zero-Copy Everywhere

Salvo keeps per-request allocations to a minimum by borrowing wherever possible: request bodies are cached as `Bytes`, path and query deserialization borrows from the URI via `Cow`, static files and Range responses serve borrowed slices, and short query keys/values are stored inline in `CompactString`.

### Serverless Without Forking Your Code

Salvo runs where there is no long-lived TCP listener. A single `#[service]` entry point generates a native handler for AWS Lambda and Cloudflare Workers:

```rust
use salvo::prelude::*;

#[service(start = init)]
fn build() -> Router {
    Router::new().push(Router::with_path("hello").get(|| async { "hello" }))
}

fn init() { /* one-time warm-start setup */ }
```

Request and response bodies stream end-to-end on both runtimes.

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
- [mTLS authentication and certificate-based authorization](./examples/mtls-rustls/)
- [MCP server using the Rust SDK and Tower compatibility](./examples/mcp/)
- [Feature-specific crates](./crates/)

## Recommended Projects

- **[Savhub](https://github.com/savhub-ai/savhub)** - Easily manage your AI skills. A platform built with Salvo for organizing and sharing AI capabilities.
- **[Palpo](https://github.com/palpo-matrix-server/palpo)** - A Matrix server implementation in Rust, powered by Salvo.

Check out the full list of community projects in our [ECOSYSTEM.md](./ECOSYSTEM.md).

## Performance

Salvo consistently ranks among the fastest Rust web frameworks:
- [Web Frameworks Benchmark](https://web-frameworks-benchmark.netlify.app/result?l=rust)
- [TechEmpower Benchmarks](https://www.techempower.com/benchmarks/#section=data-r23)

## Support

If you find Salvo useful, consider [buying me a coffee](https://ko-fi.com/chrislearn).

## License

Licensed under [Apache License 2.0](LICENSE).