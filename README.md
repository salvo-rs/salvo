<div align="center">
<img alt="Savlo" src="assets/logo.svg" />
<p>

[![build status](https://github.com/salvo-rs/salvo/workflows/ci-linux/badge.svg?branch=master&event=push)](https://github.com/salvo-rs/salvo/actions)
[![build status](https://github.com/salvo-rs/salvo//workflows/ci-macos/badge.svg?branch=master&event=push)](https://github.com/salvo-rs/salvo/actions)
[![build status](https://github.com/salvo-rs/salvo/workflows/ci-windows/badge.svg?branch=master&event=push)](https://github.com/salvo-rs/salvo/actions)
<br>
[![crates.io](https://img.shields.io/crates/v/salvo)](https://crates.io/crates/salvo)
[![Documentation](https://docs.rs/salvo/badge.svg)](https://docs.rs/salvo)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)
[![Rust Version](https://img.shields.io/badge/rust-1.56%2B-blue)](https://blog.rust-lang.org/2021/10/21/Rust-1.56.0.html)
<br>
[![codecov](https://codecov.io/gh/salvo-rs/salvo/branch/master/graph/badge.svg)](https://codecov.io/gh/salvo-rs/salvo)
[![Website](https://img.shields.io/website?down_color=lightgrey&down_message=offline&up_color=blue&up_message=online&url=https%3A%2F%2Fsalvo.rs)](https://salvo.rs)
![License](https://img.shields.io/crates/l/salvo.svg)
</p>
</div>

Salvo is a powerful and simplest web server framework in Rust world. 
## 🎯 Features
  - Base on hyper, tokio and async supported;
  - Websocket supported;
  - Middleware is handler and support executed before or after handler;
  - Easy to use routing system, routers can be nested, and you can add middlewares on any routers;
  - Multipart form supported, handle files upload is very simple;
  - Serve a static virtual directory from many physical directories;

## ⚡️ Quick start
You can view samples [here](https://github.com/salvo-rs/salvo/tree/main/examples), or view [offical website](https://salvo.rs/book/quick-start/hello_world/).

Create a new rust project:

```bash
cargo new hello_salvo --bin
```

Add this to `Cargo.toml`

```toml
[dependencies]
salvo = { version = "0.16", features = ["full"] }
tokio = { version = "1", features = ["full"] }
```

Create a simple function handler in the main.rs file, we call it `hello_world`, this function just render plain text ```"Hello World"```.

``` rust
use salvo::prelude::*;

#[fn_handler]
async fn hello_world(res: &mut Response) {
    res.render(Text::Plain("Hello World"));
}
```

In the ```main``` function, we need to create a root Router first, and then create a server and call it's ```bind``` function:

```rust
use salvo::prelude::*;

#[fn_handler]
async fn hello_world() -> &'static str {
    "Hello World"
}
#[tokio::main]
async fn main() {
    let router = Router::new().get(hello_world);
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
```

### Middleware
There is no difference between Handler and Middleware, Middleware is just Handler. **So you can write middlewares without to know concpets like associated type, generic type. You can write middleware if you can write function!!!***
### Tree-like routing system

Normally we write routing like this：

```rust
Router::with_path("articles").get(list_articles).post(create_article);
Router::with_path("articles/<id>")
    .get(show_article)
    .patch(edit_article)
    .delete(delete_article);
```

Often viewing articles and article lists does not require user login, but creating, editing, deleting articles, etc. require user login authentication permissions. The tree-like routing system in Salvo can meet this demand. We can write routers without user login together: 

```rust
Router::with_path("articles")
    .get(list_articles)
    .push(Router::with_path("<id>").get(show_article));
```

Then write the routers that require the user to login together, and use the corresponding middleware to verify whether the user is logged in: 
```rust
Router::with_path("articles")
    .hoop(auth_check)
    .post(list_articles)
    .push(Router::with_path("<id>").patch(edit_article).delete(delete_article));
```

Although these two routes have the same ```path("articles")```, they can still be added to the same parent route at the same time, so the final route looks like this: 

```rust
Router::new()
    .push(
        Router::with_path("articles")
            .get(list_articles)
            .push(Router::with_path("<id>").get(show_article)),
    )
    .push(
        Router::with_path("articles")
            .hoop(auth_check)
            .post(list_articles)
            .push(Router::with_path("<id>").patch(edit_article).delete(delete_article)),
    );
```

```<id>``` matches a fragment in the path, under normal circumstances, the article ```id``` is just a number, which we can use regular expressions to restrict ```id``` matching rules, ```r"<id:/\d+/>"```.

You can also use ```<*>``` or ```<**>``` to match all remaining path fragments. In order to make the code more readable, you can also add appropriate name to make the path semantics more clear, for example: ```<**file_path>```.

### File upload
We can get file async by the function ```get_file``` in ```Request```:

```rust
#[fn_handler]
async fn upload(req: &mut Request, res: &mut Response) {
    let file = req.get_file("file").await;
    if let Some(file) = file {
        let dest = format!("temp/{}", file.filename().unwrap_or_else(|| "file".into()));
        if let Err(e) = tokio::fs::copy(&file.path, Path::new(&dest)).await {
            res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
        } else {
            res.render(Text::Plain("Ok");
        }
    } else {
        res.set_status_code(StatusCode::BAD_REQUEST);
    }
}
```

### More Examples
Your can find more examples in [examples](./examples/) folder. You can run these examples with the following command:

```
cargo run --example basic_auth
```

You can use any example name you want to run instead of ```basic_auth``` here. 

There is a real and open source project use Salvo: [https://github.com/driftluo/myblog](https://github.com/driftluo/myblog).


## 🚀 Performance
Benchmark testing result can be found from here:

[https://web-frameworks-benchmark.netlify.app/result?l=rust](https://web-frameworks-benchmark.netlify.app/result?l=rust)

[https://www.techempower.com/benchmarks/#section=test&runid=1922b097-2d7f-413c-be21-9571c8302734&hw=ph&test=query&l=zik0zj-e6&a=2](https://www.techempower.com/benchmarks/#section=test&runid=1922b097-2d7f-413c-be21-9571c8302734&hw=ph&test=query&l=zik0zj-e6&a=2)

## 🩸 Contributing

Contributions are absolutely, positively welcome and encouraged! Contributions come in many forms. You could:

  - Submit a feature request or bug report as an issue;
  - Comment on issues that require feedback;
  - Contribute code via pull requests;
  - Publish Salvo-related technical articles on blogs or technical platforms。

All pull requests are code reviewed and tested by the CI. Note that unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Salvo by you shall be dual licensed under the MIT License, without any additional terms or conditions.

## ☕ Supporters

Salvo is an open source project. If you want to support Salvo, you can ☕ [**buy a coffee here**](https://www.buymeacoffee.com/chrislearn).

## ⚠️ License

Salvo is licensed under either of
* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))
* MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
