<div align="center">
<p><img alt="Salvo" width="132" style="max-width:40%;min-width:60px;" src="https://salvo.rs/images/logo-text.svg" /></p>

**功能强大且简单易用的 Rust Web 框架**

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

> 加入社区: 微信(chrislearn) | QQ群: 823441777

## 特性

- **简单强大** - 零样板代码，会写函数就会写 Handler
- **HTTP/1、HTTP/2、HTTP/3** - 开箱即用的全协议支持
- **灵活路由** - 树形路由，中间件可挂载到任意层级
- **自动证书** - 内置 ACME，自动获取和续期 TLS 证书
- **OpenAPI** - 一流的 OpenAPI 支持，自动生成文档
- **WebSocket & WebTransport** - 内置实时通信支持
- **基于 Hyper & Tokio** - 生产级异步运行时

## 快速开始

创建项目：

```bash
cargo new hello-salvo
cd hello-salvo
cargo add salvo tokio --features salvo/oapi,tokio/macros
```

编写 `src/main.rs`：

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

运行：

```bash
cargo run
```

## 为什么选择 Salvo？

### 中间件 = Handler

无需复杂的泛型和 trait，中间件就是普通函数：

```rust
#[handler]
async fn add_header(res: &mut Response) {
    res.headers_mut().insert(header::SERVER, HeaderValue::from_static("Salvo"));
}

Router::new().hoop(add_header).get(hello)
```

### 树形路由 + 中间件

为不同路由分支应用不同的中间件：

```rust
Router::new()
    // 公开路由
    .push(Router::with_path("articles").get(list_articles))
    // 需要认证的路由
    .push(Router::with_path("articles").hoop(auth_check).post(create_article).delete(delete_article))
```

### 一行代码支持 OpenAPI

只需将 `#[handler]` 改为 `#[endpoint]`：

```rust
#[endpoint]
async fn hello() -> &'static str {
    "Hello World"
}
```

### ACME 自动 HTTPS

自动从 Let's Encrypt 获取证书：

```rust
let listener = TcpListener::new("0.0.0.0:443")
    .acme()
    .add_domain("example.com")
    .http01_challenge(&mut router)
    .quinn("0.0.0.0:443"); // HTTP/3 支持
```

## CLI 工具

```bash
cargo install salvo-cli
salvo new my_project
```

## 了解更多

- [官方网站](https://salvo.rs)
- [API 文档](https://docs.rs/salvo)
- [示例代码](./examples/)

## 性能

Salvo 在 Rust Web 框架中性能名列前茅：

- [Web Frameworks Benchmark](https://web-frameworks-benchmark.netlify.app/result?l=rust)
- [TechEmpower Benchmarks](https://www.techempower.com/benchmarks/#section=data-r23)

## 赞助

如果 Salvo 对你有帮助，欢迎[请我喝杯咖啡](https://ko-fi.com/chrislearn)。

<p style="text-align: center;">
<img src="https://salvo.rs/images/alipay.png" alt="Alipay" width="180"/>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;<img src="https://salvo.rs/images/weixin.png" alt="Weixin" width="180"/>
</p>

## 开源协议

基于 [Apache License 2.0](LICENSE-APACHE) 或 [MIT license](LICENSE-MIT) 双协议授权。
