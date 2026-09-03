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
<a href="https://blog.rust-lang.org/2026/03/05/Rust-1.94.0/"><img alt="Rust Version" src="https://img.shields.io/badge/rust-1.94%2B-blue" /></a>
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

# salvo-otel

OpenTelemetry integration for the Salvo web framework. This crate provides middleware for collecting metrics and distributed traces using the [OpenTelemetry](https://opentelemetry.io/) observability framework.

## Features

- **Metrics collection**: Track request duration, active requests, and body sizes
- **Distributed tracing**: Add spans to requests for end-to-end visibility
- **Standard conventions**: Uses OpenTelemetry HTTP semantic conventions
- **Easy integration**: Simple middleware that works with any OpenTelemetry exporter

## Components

| Middleware | Purpose |
|------------|---------|
| `Metrics` | Records the HTTP server metrics of the semantic conventions |
| `Tracing` | Adds distributed tracing spans to requests |

## Collected Metrics

The `Metrics` middleware records:

| Name | Instrument | Unit |
|------|------------|------|
| `http.server.request.duration` | histogram | `s` |
| `http.server.active_requests` | up-down counter | `{request}` |
| `http.server.request.body.size` | histogram | `By` |
| `http.server.response.body.size` | histogram | `By` |

There is no request or error counter: the duration histogram already carries the
request count — a Prometheus exporter renders it as
`http_server_request_duration_seconds_count` — and failures are selected by
filtering on `error.type` or `http.response.status_code`.

## Attributes

Both middlewares follow the [HTTP semantic conventions][semconv] for attribute
names and values: `http.request.method`, `http.route`,
`http.response.status_code`, `url.scheme`, `url.path`, `url.query`,
`network.protocol.version` and `error.type`.

The request target is reported as the matched route template (`/users/{id}`)
rather than the requested URI, so the number of metric time series stays
proportional to the number of routes rather than growing with traffic. Requests
with a method the conventions do not name are reported as `_OTHER`; list your
own with `Metrics::with_known_methods`.

`http.route` needs the `matched-path` feature, which is enabled by default —
through the `salvo` crate, by its own `matched-path` feature.

[semconv]: https://opentelemetry.io/docs/specs/semconv/http/http-metrics/

## Installation

This is an official crate, so you can enable it in `Cargo.toml`:

```toml
salvo = { version = "*", features = ["otel"] }
```

## Quick Start

```rust
use salvo::prelude::*;
use salvo::otel::{Metrics, Tracing};

#[handler]
async fn hello() -> &'static str {
    "Hello World!"
}

#[tokio::main]
async fn main() {
    // Initialize your OpenTelemetry provider here...

    let router = Router::new()
        .hoop(Metrics::new())
        .hoop(Tracing::new())
        .get(hello);

    let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
    Server::new(acceptor).serve(router).await;
}
```

## Documentation & Resources

- [API Documentation](https://docs.rs/salvo-otel)
- [Example Projects](https://github.com/salvo-rs/salvo/tree/main/examples)

## ☕ Donate

Salvo is an open source project. If you want to support Salvo, you can ☕ [**buy me a coffee here**](https://ko-fi.com/chrislearn).

## ⚠️ License

Salvo is licensed under [Apache License, Version 2.0](LICENSE) ([http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0)).
