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

Salvo(賽風) 是一個極其簡單且功能強大的 Rust Web 後端框架。僅僅需要基礎 Rust 知識即可開發後端服務。

## 🎯 功能特色

- 基於 [Hyper 1](https://crates.io/crates/hyper), [Tokio](https://crates.io/crates/tokio) 開發;
- 統一的中間件和句柄接口;
- 支持 HTTP1, HTTP2 和 **HTTP3**;
- 路由可以無限嵌套，並且可以在任何路由中附加多個中間件;
- 集成 Multipart 表單處理;
- 支持 WebSocket, WebTransport;
- 支持 OpenAPI;
- 支持 Acme, 自動從 [let's encrypt](https://letsencrypt.org/)獲取 TLS 證書。
- 支持 Tower Service 和 Layer.

## ⚡️ 快速開始

你可以查看[實例代碼](https://github.com/salvo-rs/salvo/tree/main/examples), 或者訪問[官網](https://salvo.rs)。

### 支持 ACME 自動獲取證書和 HTTP3 的 Hello World

**只需要幾行代碼就可以實現一個同時支持 ACME 自動獲取證書以及支持 HTTP1，HTTP2，HTTP3 協議的伺服器。**

```rust
use salvo::prelude::*;

#[handler]
async fn hello(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render(Text::Plain("Hello World"));
}

#[tokio::main]
async fn main() {
    let mut router = Router::new().get(hello);
    let listener = TcpListener::new("0.0.0.0:443")
        .acme()
        .add_domain("test.salvo.rs") // 用你自己的域名替换此域名
        .http01_challenge(&mut router).quinn("0.0.0.0:443");
    let acceptor = listener.join(TcpListener::new("0.0.0.0:80")).bind().await;
    Server::new(acceptor).serve(router).await;
}
```

### 中間件

Salvo 中的中間件其實就是 Handler, 沒有其他任何特別之處。**所以書寫中間件並不需要像其他某些框架需要掌握泛型關聯類型等知識。只要你會寫函數就會寫中間件，就是這麼簡單!!!**

```rust
use salvo::http::header::{self, HeaderValue};
use salvo::prelude::*;

#[handler]
async fn add_header(res: &mut Response) {
    res.headers_mut()
        .insert(header::SERVER, HeaderValue::from_static("Salvo"));
}
```

然後將它添加到路由中：

```rust
Router::new().hoop(add_header).get(hello)
```

這就是一個簡單的中間件，它向 `Response` 的頭部添加了 `Header`, 查看[完整源碼](https://github.com/salvo-rs/salvo/blob/main/examples/middleware-add-header/src/main.rs)。

### 可鏈式書寫的樹狀路由係統

正常情況下我們是這樣寫路由的：

```rust
Router::with_path("articles").get(list_articles).post(create_article);
Router::with_path("articles/{id}")
    .get(show_article)
    .patch(edit_article)
    .delete(delete_article);
```

往往查看文章和文章列錶是不需要用戶登錄的，但是創建，編輯，刪除文章等需要用戶登錄認證權限才可以。Salvo 中支持嵌套的路由係統可以很好地滿足這種需求。我們可以把不需要用戶登錄的路由寫到一起：

```rust
Router::with_path("articles")
    .get(list_articles)
    .push(Router::with_path("{id}").get(show_article));
```

然後把需要用戶登錄的路由寫到一起，並且使用相應的中間件驗證用戶是否登錄：

```rust
Router::with_path("articles")
    .hoop(auth_check)
    .push(Router::with_path("{id}").patch(edit_article).delete(delete_article));
```

雖然這兩個路由都有這同樣的 `path("articles")`, 然而它們依然可以被同時添加到同一個父路由，所以最後的路由長成了這個樣子：

```rust
Router::new()
    .push(
        Router::with_path("articles")
            .get(list_articles)
            .push(Router::with_path("{id}").get(show_article)),
    )
    .push(
        Router::with_path("articles")
            .hoop(auth_check)
            .push(Router::with_path("{id}").patch(edit_article).delete(delete_article)),
    );
```

`{id}`匹配了路徑中的一個片段，正常情況下文章的 `id`隻是一個數字，這是我們可以使用正則表達式限制 `id`的匹配規則，`r"{id|\d+}"`。

還可以通過 `{**}`, `{*+}` 或者 `{*?}`匹配所有剩餘的路徑片段。為了代碼易讀性性強些，也可以添加適合的名字，讓路徑語義更清晰，比如：: `{**file_path}`。

有些用於匹配路徑的正則表達式需要經常被使用，可以將它事先註冊，比如 GUID:

```rust
PathFilter::register_wisp_regex(
    "guid",
    Regex::new("[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}").unwrap(),
);
```

這樣在需要路徑匹配時就變得更簡潔：

```rust
Router::with_path("{id:guid}").get(index)
```

查看[完整源碼](https://github.com/salvo-rs/salvo/blob/main/examples/routing-guid/src/main.rs)

### 文件上傳

可以通過 `Request` 中的 `file`異步獲取上傳的文件：

```rust
#[handler]
async fn upload(req: &mut Request, res: &mut Response) {
    let file = req.file("file").await;
    if let Some(file) = file {
        let dest = format!("temp/{}", file.name().unwrap_or_else(|| "file".into()));
        if let Err(e) = std::fs::copy(&file.path, Path::new(&dest)) {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        } else {
            res.render("Ok");
        }
    } else {
        res.status_code(StatusCode::BAD_REQUEST);
    }
}
```

### 提取請求數據

可以輕鬆地從多個不同數據源獲取數據，並且組裝為你想要的類型。可以先定義一個自定義的類型，比如：

```rust
#[derive(Serialize, Deserialize, Extractible, Debug)]
/// 默認從 body 中獲取數據字段值
#[salvo(extract(default_source(from = "body")))]
struct GoodMan<'a> {
    /// 其中, id 號從請求路徑參數中獲取, 並且自動解析數據為 i64 類型.
    #[salvo(extract(source(from = "param")))]
    id: i64,
    /// 可以使用引用類型, 避免內存複製.
    username: &'a str,
    first_name: String,
    last_name: String,
}
```

然後在 `Handler`中可以這樣獲取數據：

```rust
#[handler]
async fn edit(req: &mut Request) {
    let good_man: GoodMan<'_> = req.extract().await.unwrap();
}
```

甚至於可以直接把類型作為參數傳入函數，像這樣：

```rust
#[handler]
async fn edit<'a>(good_man: GoodMan<'a>) {
    res.render(Json(good_man));
}
```

查看[完整源碼](https://github.com/salvo-rs/salvo/blob/main/examples/extract-nested/src/main.rs)

### OpenAPI 支持

無需對項目做大的改動，即可實現對 OpenAPI 的完美支持。

```rust
#[derive(Serialize, Deserialize, ToSchema, Debug)]
struct MyObject<T: ToSchema + std::fmt::Debug> {
    value: T,
}

#[endpoint]
async fn use_string(body: JsonBody<MyObject<String>>) -> String {
    format!("{:?}", body)
}
#[endpoint]
async fn use_i32(body: JsonBody<MyObject<i32>>) -> String {
    format!("{:?}", body)
}
#[endpoint]
async fn use_u64(body: JsonBody<MyObject<u64>>) -> String {
    format!("{:?}", body)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .push(Router::with_path("i32").post(use_i32))
        .push(Router::with_path("u64").post(use_u64))
        .push(Router::with_path("string").post(use_string));

    let doc = OpenApi::new("test api", "0.0.1").merge_router(&router);

    let router = router
        .unshift(doc.into_router("/api-doc/openapi.json"))
        .unshift(SwaggerUi::new("/api-doc/openapi.json").into_router("swagger-ui"));

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
```

### 🛠️ Salvo CLI

Salvo CLI 是一個命令行工具，可以簡化創建新的 Salvo 項目的過程，支援 Web API、網站、資料庫（包括透過 SQLx、SeaORM、Diesel、Rbatis 支援的 SQLite、PostgreSQL、MySQL）和基本的中介軟體的模板。
你可以使用 [salvo-cli](https://github.com/salvo-rs/salvo-cli)来來創建一個新的 Salvo 項目：

#### 安裝

```bash
cargo install salvo-cli
```

#### 創建一個新的 Salvo 項目

```bash
salvo new project_name
```

___

### 更多示例

您可以從 [examples](./examples/)文件夾下查看更多示例代碼，您可以通過以下命令運行這些示例：


```bash
cd examples
cargo run --bin example-basic-auth
```

您可以使用任何你想運行的示例名稱替代這裏的 `basic-auth`。

## 🚀 性能

Benchmark 測試結果可以從這裏查看：

[https://web-frameworks-benchmark.netlify.app/result?l=rust](https://web-frameworks-benchmark.netlify.app/result?l=rust)

[https://www.techempower.com/benchmarks/#section=data-r22](https://www.techempower.com/benchmarks/#section=data-r22)

## 🩸 貢獻者

<a href="https://github.com/salvo-rs/salvo/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=salvo-rs/salvo" />
</a>

## ☕ 捐助

`Salvo`是一個開源項目，如果想支持本項目，可以 ☕ [**請我喝杯咖啡**](https://ko-fi.com/chrislearn)。
<p style="text-align: center;">
<img src="https://salvo.rs/images/alipay.png" alt="Alipay" width="180"/>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;<img src="https://salvo.rs/images/weixin.png" alt="Weixin" width="180"/>
</p>

## ⚠️ 開源協議

Salvo 項目採用以下開源協議：

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

- MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))