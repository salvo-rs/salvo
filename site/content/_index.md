+++
title = "Home"
weight = 1
+++

<div align="center">
<img alt="Savlo" src="site/static/logo.svg" />
<p>

[![build status](https://github.com/kenorld/salvo/workflows/CI%20(Linux)/badge.svg?branch=master&event=push)](https://github.com/kenorld/salvo/actions)
[![build status](https://github.com/kenorld/salvo//workflows/CI%20(macOS)/badge.svg?branch=master&event=push)](https://github.com/kenorld/salvo/actions)
[![build status](https://github.com/kenorld/salvo/workflows/CI%20(Windows)/badge.svg?branch=master&event=push)](https://github.com/kenorld/salvo/actions)
<br>
[![codecov](https://codecov.io/gh/kenorld/salvo/branch/master/graph/badge.svg)](https://codecov.io/gh/kenorld/salvo)
[![crates.io](https://img.shields.io/crates/v/salvo)](https://crates.io/crates/salvo)
[![Download](https://img.shields.io/crates/d/salvo.svg)](https://crates.io/crates/salvo)
![License](https://img.shields.io/crates/l/salvo.svg)
</p>
<h3>Salvo is a easy to use web framework written by rust.</h3>
</div>

## 🎯 Features
  * Base on latest hyper, tokio;
  * Websocket supported;
  * Serve a static virtual directory from many physical directories.
  * Middlewares support executed before or after handle.
  * Easy routing:
    - Path parameters and regex supported;
    - Tree-like routing system;

## ⚡️ Quick start
You can view samples [here](https://github.com/kenorld/salvo/tree/master/examples) or read docs [here](https://docs.rs/salvo/).

Create a new rust project:
```bash
cargo new hello_salvo --bin
```

Add this to `Cargo.toml`
```toml
[dependencies]
salvo = "0.4"
tokio = { version = "1.0", features = ["full"] }
```

Create a simple function handler in the main.rs file, we call it `hello_world`, this function just render plain text "Hello World".

```rust
use salvo::prelude::*;

#[fn_handler]
async fn hello_world(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render_plain_text("Hello World");
}
```

In the ```main``` function, we need to create a root Router first, and then create a server and call it's ```bind``` function:

```rust
use salvo::prelude::*;

#[fn_handler]
async fn hello_world(res: &mut Response) {
    res.render_plain_text("Hello World");
}

#[tokio::main]
async fn main() {
    let router = Router::new().get(hello_world);
    let server = Server::new(router);
    server.bind(([0, 0, 0, 0], 7878)).await;
}
```
### Tree-like routing system

```rust
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    let router = Router::new()
        .get(index)
        .push(
            Router::new()
                .path("users")
                .before(auth)
                .post(create_user)
                .push(Router::new().path(r"<id:/\d+/>").post(update_user).delete(delete_user)),
        )
        .push(
            Router::new()
                .path("users")
                .get(list_users)
                .push(Router::new().path(r"<id:/\d+/>").get(show_user)),
        );

    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}

#[fn_handler]
async fn index(res: &mut Response) {
    res.render_plain_text("Hello world!");
}
#[fn_handler]
async fn auth(res: &mut Response) {
    res.render_plain_text("user has authed\n\n");
}
#[fn_handler]
async fn list_users(res: &mut Response) {
    res.render_plain_text("list users");
}
#[fn_handler]
async fn show_user(res: &mut Response) {
    res.render_plain_text("show user");
}
#[fn_handler]
async fn create_user(res: &mut Response) {
    res.render_plain_text("user created");
}
#[fn_handler]
async fn update_user(res: &mut Response) {
    res.render_plain_text("user updated");
}
#[fn_handler]
async fn delete_user(res: &mut Response) {
    res.render_plain_text("user deleted");
}

```

### More Examples
Your can find more examples in [examples](./examples/) folder:
- [basic_auth.rs](./examples/basic_auth.rs)
- [compression.rs](./examples/compression.rs)
- [file_list.rs](./examples/file_list.rs)
- [routing.rs](./examples/routing.rs)
- [sse_chat.rs](./examples/sse_chat.rs)
- [sse.rs](./examples/sse.rs)
- [tls.rs](./examples/tls.rs)
- [todos.rs](./examples/todos.rs)
- [unix_socket.rs](./examples/unix_socket.rs)
- [ws_chat.rs](./examples/ws_chat.rs)
- [ws.rs](./examples/ws.rs)

## ☕ Supporters

Salvo is an open source project. If you want to support Salvo, you can ☕ [**buy a coffee here**](https://www.buymeacoffee.com/chrislearn).

## ⚠️ License

Salvo is licensed under MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)