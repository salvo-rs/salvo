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

Salvo(赛风) 是一个极其简单且功能强大的 Rust Web 后端框架。仅仅需要基础 Rust 知识即可开发后端服务。

> 中国用户可以添加我微信 (chrislearn), 拉微信讨论群或者直接加入 QQ 群：823441777.
>
> 中国同步仓库：
> - Gitee: https://gitee.com/salvo-rs/salvo
> - Gitcode: https://gitcode.com/salvo-rs/salvo

## 🎯 功能特色

- 基于 [Hyper 1](https://crates.io/crates/hyper), [Tokio](https://crates.io/crates/tokio) 开发;
- 支持 HTTP1, HTTP2 和 **HTTP3**;
- 统一的中间件和句柄接口;
- 路由可以无限嵌套，并且可以在任何路由中附加多个中间件;
- 集成 Multipart 表单处理;
- 支持 WebSocket, WebTransport;
- 支持 OpenAPI;
- 支持 Acme, 自动从 [let's encrypt](https://letsencrypt.org/)获取 TLS 证书。
- 支持 Tower Service 和 Layer.

## ⚡️ 快速开始

你可以查看[实例代码](https://github.com/salvo-rs/salvo/tree/main/examples), 或者访问[官网](https://salvo.rs)。

### 支持 ACME 自动获取证书和 HTTP3 的 Hello World

**只需要几行代码就可以实现一个同时支持 ACME 自动获取证书以及支持 HTTP1，HTTP2，HTTP3 协议的服务器。**

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

### 中间件

Salvo 中的中间件其实就是 Handler, 没有其他任何特别之处。**所以书写中间件并不需要像其他某些框架需要掌握泛型关联类型等知识。只要你会写函数就会写中间件，就是这么简单!!!**

```rust
use salvo::http::header::{self, HeaderValue};
use salvo::prelude::*;

#[handler]
async fn add_header(res: &mut Response) {
    res.headers_mut()
        .insert(header::SERVER, HeaderValue::from_static("Salvo"));
}
```

然后将它添加到路由中：

```rust
Router::new().hoop(add_header).get(hello)
```

这就是一个简单的中间件，它向 `Response` 的头部添加了 `Header`, 查看[完整源码](https://github.com/salvo-rs/salvo/blob/main/examples/middleware-add-header/src/main.rs)。

### 可链式书写的树状路由系统

正常情况下我们是这样写路由的：

```rust
Router::with_path("articles").get(list_articles).post(create_article);
Router::with_path("articles/{id}")
    .get(show_article)
    .patch(edit_article)
    .delete(delete_article);
```

往往查看文章和文章列表是不需要用户登录的，但是创建，编辑，删除文章等需要用户登录认证权限才可以。Salvo 中支持嵌套的路由系统可以很好地满足这种需求。我们可以把不需要用户登录的路由写到一起：

```rust
Router::with_path("articles")
    .get(list_articles)
    .push(Router::with_path("{id}").get(show_article));
```

然后把需要用户登录的路由写到一起，并且使用相应的中间件验证用户是否登录：

```rust
Router::with_path("articles")
    .hoop(auth_check)
    .push(Router::with_path("{id}").patch(edit_article).delete(delete_article));
```

虽然这两个路由都有着同样的 `path("articles")`, 然而它们依然可以被同时添加到同一个父路由，所以最后的路由长成了这个样子：

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

`{id}`匹配了路径中的一个片段，正常情况下文章的 `id`只是一个数字，这时我们可以使用正则表达式限制 `id`的匹配规则，`r"{id|\d+}"`。

还可以通过 `{**}`, `{*+}` 或者 `{*?}`匹配所有剩余的路径片段。为了代码易读性强些，也可以添加适合的名字，让路径语义更清晰，比如：: `{**file_path}`。

有些用于匹配路径的正则表达式需要经常被使用，可以将它事先注册，比如 GUID:

```rust
PathFilter::register_wisp_regex(
    "guid",
    Regex::new("[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}").unwrap(),
);
```

这样在需要路径匹配时就变得更简洁：

```rust
Router::with_path("{id:guid}").get(index)
```

查看[完整源码](https://github.com/salvo-rs/salvo/blob/main/examples/routing-guid/src/main.rs)

### 文件上传

可以通过 `Request` 中的 `file`异步获取上传的文件：

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

### 提取请求数据

可以轻松地从多个不同数据源获取数据，并且组装为你想要的类型。可以先定义一个自定义的类型，比如：

```rust
#[derive(Serialize, Deserialize, Extractible, Debug)]
/// 默认从 body 中获取数据字段值
#[salvo(extract(default_source(from = "body")))]
struct GoodMan<'a> {
    /// 其中, id 号从请求路径参数中获取, 并且自动解析数据为 i64 类型.
    #[salvo(extract(source(from = "param")))]
    id: i64,
    /// 可以使用引用类型, 避免内存复制.
    username: &'a str,
    first_name: String,
    last_name: String,
}
```

然后在 `Handler`中可以这样获取数据：

```rust
#[handler]
async fn edit(req: &mut Request) {
    let good_man: GoodMan<'_> = req.extract().await.unwrap();
}
```

甚至于可以直接把类型作为参数传入函数，像这样：

```rust
#[handler]
async fn edit<'a>(good_man: GoodMan<'a>) {
    res.render(Json(good_man));
}
```

查看[完整源码](https://github.com/salvo-rs/salvo/blob/main/examples/extract-nested/src/main.rs)

### OpenAPI 支持

无需对项目做大的改动，即可实现对 OpenAPI 的完美支持。

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

Salvo CLI 是一个命令行工具，可以简化创建新的 Salvo 项目的过程，支持 Web API、网站、数据库（包括通过 SQLx、SeaORM、Diesel、Rbatis 支持的 SQLite、PostgreSQL、MySQL）和基本的中间件的模板。
你可以使用 [salvo-cli](https://github.com/salvo-rs/salvo-cli)来创建一个新的 Salvo 项目：

#### 安装

```bash
cargo install salvo-cli
```

#### 创建一个 Salvo 项目

```bash
salvo new project_name
```

___

### 更多示例

您可以从 [examples](./examples/)文件夹下查看更多示例代码，您可以通过以下命令运行这些示例：

```bash
cd examples
cargo run --bin example-basic-auth
```

您可以使用任何你想运行的示例名称替代这里的 `basic-auth`。

## 🚀 性能

Benchmark 测试结果可以从这里查看：

[https://web-frameworks-benchmark.netlify.app/result?l=rust](https://web-frameworks-benchmark.netlify.app/result?l=rust)

[https://www.techempower.com/benchmarks/#section=data-r22](https://www.techempower.com/benchmarks/#section=data-r22)

## 🩸 贡献者

<a href="https://github.com/salvo-rs/salvo/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=salvo-rs/salvo" />
</a>

## ☕ 捐助

`Salvo`是一个开源项目，如果想支持本项目，可以 ☕ [**请我喝杯咖啡**](https://ko-fi.com/chrislearn)。
<p style="text-align: center;">
<img src="https://salvo.rs/images/alipay.png" alt="Alipay" width="180"/>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;<img src="https://salvo.rs/images/weixin.png" alt="Weixin" width="180"/>
</p>

## ⚠️ 开源协议

Salvo 项目采用以下开源协议：

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

- MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))

