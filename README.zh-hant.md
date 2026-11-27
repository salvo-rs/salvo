<div align="center">
<p><img alt="Salvo" width="132" style="max-width:40%;min-width:60px;" src="https://salvo.rs/images/logo-text.svg" /></p>

**功能強大且簡單易用的 Rust Web 框架**

<p>
    <a href="https://github.com/Czlz/salvo/blob/feat/serverless/README.md">English</a>&nbsp;&nbsp;
    <a href="https://github.com/Czlz/salvo/blob/feat/serverless/README.zh.md">简体中文</a>&nbsp;&nbsp;
    <a href="https://github.com/Czlz/salvo/blob/feat/serverless/README.zh-hant.md">繁體中文</a>
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

## 與官方上游專案的差異

本專案是 [salvo-rs/salvo](https://github.com/salvo-rs/salvo) 的一個 fork，維護於 `feat/serverless` 分支。它在保持與上游公共 API 相容的同時，額外提供了以下能力：

### 1. 無伺服器執行階段（AWS Lambda + Cloudflare Workers）

- 新增 `salvo-serverless` crate，讓同一套 `Router` 零改動適配 **AWS Lambda** 與 **Cloudflare Workers** 兩種執行階段。
- `#[service]` 屬性巨集把零參數的 Router 建構函式轉換為平台入口（Lambda handler 或 Workers 的 `#[worker::event]`），並支援可選的 `start = <path>` 一次性預熱初始化。
- 透過 `rt_lambda` 與 `rt_worker` 特性分別啟用對應執行階段；Workers 適配器編譯到 `wasm32-unknown-unknown`。
- 請求與回應體在兩種執行階段上端到端串流轉發。Workers 適配器還暴露了 Cloudflare 綁定（D1、R2、Queue、KV），並提供 WebSocket 相容層，讓現有 `WebSocketUpgrade` 處理器無需改動即可在 `wasm32` 上執行。

### 2. 效能優化：零拷貝請求管線

- 請求體以引用計數的 `bytes::Bytes` 快取，而非每個請求分配 `Vec<u8>`。
- 路徑與查詢反序列化透過 `Cow<'_, str>` 借用 URI，避免拷貝。
- 靜態檔案與 HTTP `Range` 回應直接回傳借用切片。
- 短查詢鍵值以內聯方式儲存在 `CompactString` 中。

### 3. 工程化與 CI

- 專門的 Serverless CI 工作流程在每次提交時把 Workers 範例交叉編譯到 `wasm32-unknown-unknown`。
- `upstream-watch` 工作流程在上游 `main` 有更新時自動建立 issue，便於追蹤與 rebase。
- 更完善的測試、基準與安全稽核套件。

## 特性

- **簡單強大** - 零樣板程式碼，會寫函數就會寫 Handler
- **HTTP/1、HTTP/2、HTTP/3** - 開箱即用的全協議支援
- **靈活路由** - 樹形路由，中介軟體可掛載到任意層級
- **自動憑證** - 內建 ACME，自動獲取和續期 TLS 憑證
- **OpenAPI** - 一流的 OpenAPI 支援，自動生成文件
- **WebSocket & WebTransport** - 內建即時通訊支援
- **零拷貝設計** - 請求解析、回應體與靜態檔案均借用 `Bytes`/`Cow`，避免冗餘記憶體分配
- **無伺服器就緒** - 同一套 `Router` 零改動即可部署到 AWS Lambda 與 Cloudflare Workers
- **基於 Hyper & Tokio** - 生產級非同步運行時

## 快速開始

建立專案：

```bash
cargo new hello-salvo
cd hello-salvo
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

### JSON API

Handler 可以反序列化請求體，並返回型別化的 JSON 回應：

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

這種寫法需要啟用 Serde 的 derive 功能：

```bash
cargo add serde --features derive
```

### 無處不在的零拷貝

Salvo 透過盡可能借用來最小化每請求的記憶體分配：請求體以 `Bytes` 快取，路徑與查詢反序列化透過 `Cow` 借用 URI，靜態檔案與 Range 回應直接返回借用切片，短查詢鍵值則以 `CompactString` 內聯儲存。

### 無需改寫的 Serverless

Salvo 在沒有長駐 TCP 監聽器的環境同樣可用。單個 `#[service]` 入口即可為 AWS Lambda 與 Cloudflare Workers 生成原生處理器：

```rust
use salvo::prelude::*;

#[service(start = init)]
fn build() -> Router {
    Router::new().push(Router::with_path("hello").get(|| async { "hello" }))
}

fn init() { /* 一次性熱啟動初始化 */ }
```

兩個執行階段的請求體與回應體均端對端串流轉送。

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
- [mTLS 認證與基於用戶端憑證的授權](./examples/mtls-rustls/)
- [按功能拆分的 crate](./crates/)

## 推薦專案

- **[Savhub](https://github.com/savhub-ai/savhub)** - 輕鬆管理你的 AI 技能。基於 Salvo 構建的 AI 能力管理和分享平台。
- **[Palpo](https://github.com/palpo-matrix-server/palpo)** - 使用 Rust 實現的 Matrix 伺服器，基於 Salvo 構建。

查看完整的社區專案列表：[ECOSYSTEM.md](./ECOSYSTEM.md)。

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

基於 [Apache License 2.0](LICENSE) 授權。