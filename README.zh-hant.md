<div align="center">
<p><img alt="Salvo" width="132" style="max-width:40%;min-width:60px;" src="https://salvo.rs/images/logo-text.svg" /></p>

**功能強大且簡單易用的 Rust Web 框架**

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

## 特性

- **簡單強大** - 零樣板程式碼，會寫函數就會寫 Handler
- **HTTP/1、HTTP/2、HTTP/3** - 開箱即用的全協議支援
- **靈活路由** - 樹形路由，中介軟體可掛載到任意層級
- **自動憑證** - 內建 ACME，自動獲取和續期 TLS 憑證
- **OpenAPI** - 一流的 OpenAPI 支援，自動生成文件
- **WebSocket & WebTransport** - 內建即時通訊支援
- **基於 Hyper & Tokio** - 生產級非同步運行時

## 快速開始

建立專案：

```bash
cargo new hello-salvo && cd hello-salvo
cargo add salvo tokio --features salvo/oapi,tokio/macros
```

編寫 `src/main.rs`：

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

執行：

```bash
cargo run
```

## 為什麼選擇 Salvo？

### 中介軟體 = Handler

無需複雜的泛型和 trait，中介軟體就是普通函數：

```rust
#[handler]
async fn add_header(res: &mut Response) {
    res.headers_mut().insert(header::SERVER, HeaderValue::from_static("Salvo"));
}

Router::new().hoop(add_header).get(hello)
```

### 樹形路由 + 中介軟體

為不同路由分支應用不同的中介軟體：

```rust
Router::new()
    // 公開路由
    .push(Router::with_path("articles").get(list_articles))
    // 需要認證的路由
    .push(Router::with_path("articles").hoop(auth_check).post(create_article).delete(delete_article))
```

### 一行程式碼支援 OpenAPI

只需將 `#[handler]` 改為 `#[endpoint]`：

```rust
#[endpoint]
async fn hello() -> &'static str {
    "Hello World"
}
```

### ACME 自動 HTTPS

自動從 Let's Encrypt 獲取憑證：

```rust
let listener = TcpListener::new("0.0.0.0:443")
    .acme()
    .add_domain("example.com")
    .http01_challenge(&mut router)
    .quinn("0.0.0.0:443"); // HTTP/3 支援
```

## CLI 工具

```bash
cargo install salvo-cli
salvo new my_project
```

## 了解更多

- [官方網站](https://salvo.rs)
- [API 文件](https://docs.rs/salvo)
- [範例程式碼](./examples/)

## 效能

Salvo 在 Rust Web 框架中效能名列前茅：

- [Web Frameworks Benchmark](https://web-frameworks-benchmark.netlify.app/result?l=rust)
- [TechEmpower Benchmarks](https://www.techempower.com/benchmarks/#section=data-r23)

## 贊助

如果 Salvo 對你有幫助，歡迎[請我喝杯咖啡](https://ko-fi.com/chrislearn)。

<p style="text-align: center;">
<img src="https://salvo.rs/images/alipay.png" alt="Alipay" width="180"/>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;<img src="https://salvo.rs/images/weixin.png" alt="Weixin" width="180"/>
</p>

## 開源協議

基於 [Apache License 2.0](LICENSE-APACHE) 或 [MIT license](LICENSE-MIT) 雙協議授權。
