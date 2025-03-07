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
<a href="https://gurubase.io/g/salvo"><img alt="Gurubase" src="https://img.shields.io/badge/Gurubase-Ask%20Salvo%20Guru-006BFF" /></a>
</p>
</div>

Salvo is an extremely simple and powerful Rust web backend framework. Only basic Rust knowledge is required to develop backend services.

## 🎯 Features

- Built with [Hyper 1](https://crates.io/crates/hyper) and [Tokio](https://crates.io/crates/tokio);
- HTTP1, HTTP2 and **HTTP3**;
- Unified middleware and handle interface;
- Router can be nested infinitely, and multiple middlewares can be attached to any router;
- Integrated Multipart form processing;
- Support WebSocket, WebTransport;
- Support OpenAPI, generate OpenAPI data automatic;
- Support Acme, automatically get TLS certificate from [let's encrypt](https://letsencrypt.org/);
- Support Tower Service and Layer;

## ⚡️ Quick Start

You can view samples [here](https://github.com/salvo-rs/salvo/tree/main/examples), or view [official website](https://salvo.rs).

### Hello World with ACME and HTTP3

**It only takes a few lines of code to implement a server that supports ACME to automatically obtain certificates, and it
supports HTTP1, HTTP2, and HTTP3 protocols.**

```rust
use salvo::prelude::*;

#[handler]
async fn hello(res: &mut Response) {
    res.render(Text::Plain("Hello World"));
}

#[tokio::main]
async fn main() {
    let mut router = Router::new().get(hello);
    let listener = TcpListener::new("0.0.0.0:443")
        .acme()
        .add_domain("test.salvo.rs") // Replace this domain name with your own.
        .http01_challenge(&mut router).quinn("0.0.0.0:443");
    let acceptor = listener.join(TcpListener::new("0.0.0.0:80")).bind().await;
    Server::new(acceptor).serve(router).await;
}
```

### Middleware

There is no difference between a Handler and a Middleware, A Middleware is just a Handler. **You can write middleware
without knowing concepts like associated types and generic types. If you can write a function, then you can write middleware!!!**

```rust
use salvo::http::header::{self, HeaderValue};
use salvo::prelude::*;

#[handler]
async fn add_header(res: &mut Response) {
    res.headers_mut()
        .insert(header::SERVER, HeaderValue::from_static("Salvo"));
}
```

Then add it to router:

```rust
Router::new().hoop(add_header).get(hello)
```

This is a very simple middleware, it adds a `Header` to the `Response`, view [full source code](https://github.com/salvo-rs/salvo/blob/main/examples/middleware-add-header/src/main.rs).

### Chainable tree routing system

Normally we write routing like this:

```rust
Router::with_path("articles").get(list_articles).post(create_article);
Router::with_path("articles/{id}")
    .get(show_article)
    .patch(edit_article)
    .delete(delete_article);
```

Often, something like viewing articles and article lists does not require user login, but creating, editing, deleting articles, etc. require user login authentication permissions. The tree-like routing system in Salvo can meet this demand. We can write routers without user login together:

```rust
Router::with_path("articles")
    .get(list_articles)
    .push(Router::with_path("{id}").get(show_article));
```

Then write the routers that require the user to login together, and use the corresponding middleware to verify whether the user is logged in:

```rust
Router::with_path("articles")
    .hoop(auth_check)
    .push(Router::with_path("{id}").patch(edit_article).delete(delete_article));
```

Although these two routes have the same
`path("articles")`, they can still be added to the same parent route at the same time, so the final route looks like this:

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

`{id}` matches a fragment in the path, under normal circumstances, the article`id` is just a number, which we can use regular expressions to restrict `id` matching rules, `r"{id|\d+}"`.

You can also use `{**}`,  `{*+}` or`{*?}` to match all remaining path fragments.
In order to make the code more readable, you can also add appropriate name to make the path semantics more clear, for example: `{**file_path}`.

Some regular expressions for matching paths need to be used frequently, and it can be registered in advance, such as GUID:

```rust
PathFilter::register_wisp_regex(
    "guid",
    Regex::new("[0-9a-fA-F]{8}-([0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}").unwrap(),
);
```

This makes it more concise when path matching is required:

```rust
Router::with_path("{id:guid}").get(index)
```

View [full source code](https://github.com/salvo-rs/salvo/blob/main/examples/routing-guid/src/main.rs)

### File upload

We can get file async by the function `file` in `Request`:

```rust
#[handler]
async fn upload(req: &mut Request, res: &mut Response) {
    let file = req.file("file").await;
    if let Some(file) = file {
        let dest = format!("temp/{}", file.name().unwrap_or_else(|| "file".into()));
        if let Err(e) = tokio::fs::copy(&file.path, Path::new(&dest)).await {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        } else {
            res.render("Ok");
        }
    } else {
        res.status_code(StatusCode::BAD_REQUEST);
    }
}
```

### Extract data from request

You can easily get data from multiple different data sources and assemble it into the type you want. You can define a custom type first, for example:

```rust
#[derive(Serialize, Deserialize, Extractible, Debug)]
/// Get the data field value from the body by default.
#[salvo(extract(default_source(from = "body")))]
struct GoodMan<'a> {
    /// The id number is obtained from the request path parameter, and the data is automatically parsed as i64 type.
    #[salvo(extract(source(from = "param")))]
    id: i64,
    /// Reference types can be used to avoid memory copying.
    username: &'a str,
    first_name: String,
    last_name: String,
}
```

Then in `Handler` you can get the data like this:

```rust
#[handler]
async fn edit(req: &mut Request) {
    let good_man: GoodMan<'_> = req.extract().await.unwrap();
}
```

You can even pass the type directly to the function as a parameter, like this:

```rust
#[handler]
async fn edit<'a>(good_man: GoodMan<'a>) {
    res.render(Json(good_man));
}
```

View [full source code](https://github.com/salvo-rs/salvo/blob/main/examples/extract-nested/src/main.rs)

### OpenAPI Supported

Perfect support for OpenAPI can be achieved without making significant changes to the project.

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
        .push(doc.into_router("/api-doc/openapi.json"))
        .push(SwaggerUi::new("/api-doc/openapi.json").into_router("swagger-ui"));

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
```

### 🛠️ Salvo CLI

Salvo CLI is a command-line tool that simplifies the creation of new Salvo projects, supporting templates for web APIs, websites, databases (including SQLite, PostgreSQL, and MySQL via SQLx, SeaORM, Diesel, Rbatis), and basic middleware.
You can use [salvo-cli](https://github.com/salvo-rs/salvo-cli) to create a new Salvo project:

#### install

```bash
cargo install salvo-cli
```

#### create a new Salvo project

```bash
salvo new project_name
```

___

### More Examples

You can find more examples in [examples](./examples/) folder. You can run these examples with the following command:

```bash
cd examples
cargo run --bin example-basic-auth
```

You can use any example name you want to run instead of `basic-auth` here.

## 🚀 Performance

Benchmark testing result can be found from here:

[https://web-frameworks-benchmark.netlify.app/result?l=rust](https://web-frameworks-benchmark.netlify.app/result?l=rust)

[https://www.techempower.com/benchmarks/#section=data-r22](https://www.techempower.com/benchmarks/#section=data-r22)

## 🩸 Contributors

<a href="https://github.com/salvo-rs/salvo/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=salvo-rs/salvo" />
</a>

## ☕ Donate

Salvo is an open source project. If you want to support Salvo, you can ☕ [**buy me a coffee here**](https://ko-fi.com/chrislearn).

## ⚠️ License

Salvo is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0)).

- MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT)).
