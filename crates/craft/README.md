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
<a href="https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html"><img alt="Rust Version" src="https://img.shields.io/badge/rust-1.85%2B-blue" /></a>
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

# salvo-craft

[`Salvo`](https://github.com/salvo-rs/salvo) `Handler` modular craft macros.

## `#[craft]`

`#[craft]` is an attribute macro that converts methods in an `impl` block into [`Salvo`'s `Handler`](https://github.com/salvo-rs/salvo) implementations.

```rust
use salvo::oapi::extract::*;
use salvo::prelude::*;
use salvo_craft::craft;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let service = Arc::new(Service::new(1));
    let router = Router::new()
        .push(Router::with_path("add1").get(service.add1()))
        .push(Router::with_path("add2").get(service.add2()))
        .push(Router::with_path("add3").get(Service::add3()));
    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}

#[derive(Clone)]
pub struct Service {
    state: i64,
}

#[craft]
impl Service {
    fn new(state: i64) -> Self {
        Self { state }
    }
    /// doc line 1
    /// doc line 2
    #[craft(handler)]
    fn add1(&self, left: QueryParam<i64>, right: QueryParam<i64>) -> String {
        (self.state + *left + *right).to_string()
    }
    /// doc line 3
    /// doc line 4
    #[craft(handler)]
    pub(crate) fn add2(
        self: ::std::sync::Arc<Self>,
        left: QueryParam<i64>,
        right: QueryParam<i64>,
    ) -> String {
        (self.state + *left + *right).to_string()
    }
    /// doc line 5
    /// doc line 6
    #[craft(handler)]
    pub fn add3(left: QueryParam<i64>, right: QueryParam<i64>) -> String {
        (*left + *right).to_string()
    }
}
```

Note: `#[craft(handler)]` can be replaced with `#[craft(endpoint(...))]` for more configuration options.

NOTE: When using `&self` as the method receiver, the containing type must implement the `Clone` trait.

## Documentation & Resources

- [API Documentation](https://docs.rs/salvo-craft)
- [Example Projects](https://github.com/salvo-rs/salvo/tree/main/examples)

## ☕ Donate

Salvo is an open source project. If you want to support Salvo, you can ☕ [**buy me a coffee here**](https://ko-fi.com/chrislearn).

## ⚠️ License

Salvo is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0)).

- MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT)).
