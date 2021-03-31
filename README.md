<div align="center">
<img alt="Savlo" src="assets/logo.svg" />
<p>

[![build status](https://github.com/salvo-rs/salvo/workflows/CI%20(Linux)/badge.svg?branch=master&event=push)](https://github.com/salvo-rs/salvo/actions)
[![build status](https://github.com/salvo-rs/salvo//workflows/CI%20(macOS)/badge.svg?branch=master&event=push)](https://github.com/salvo-rs/salvo/actions)
[![build status](https://github.com/salvo-rs/salvo/workflows/CI%20(Windows)/badge.svg?branch=master&event=push)](https://github.com/salvo-rs/salvo/actions)
<br>
[![codecov](https://codecov.io/gh/salvo-rs/salvo/branch/master/graph/badge.svg)](https://codecov.io/gh/salvo-rs/salvo)
[![crates.io](https://img.shields.io/crates/v/salvo)](https://crates.io/crates/salvo)
[![Download](https://img.shields.io/crates/d/salvo.svg)](https://crates.io/crates/salvo)
![License](https://img.shields.io/crates/l/salvo.svg)
</p>
</div>

Salvo is a web server framework written in Rust.
## 🎯 Features
  * Base on hyper, tokio and async supported;
  * Websocket supported;
  * Middleware is handler and support executed before or after handle;
  * Easy to use routing system, routers can be nested, and you can add middleware in routers;
  * multipart form supported, handle files upload is very simple;
  * Serve a static virtual directory from many physical directories;

## ⚡️ Quick start
You can view samples [here](https://github.com/salvo-rs/salvo/tree/master/examples) or read docs [here](https://docs.rs/salvo/).

Create a new rust project:

```bash
cargo new hello_salvo --bin
```

Add this to `Cargo.toml`

```toml
[dependencies]
salvo = "0.9"
tokio = { version = "1", features = ["full"] }
```

Create a simple function handler in the main.rs file, we call it `hello_world`, this function just render plain text ```"Hello World"```.

```rust
use salvo::prelude::*;

#[fn_handler]
async fn hello_world(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render_plain_text("Hello World");
}
```

There are many ways to write function handler.
- You can omit function arguments if they do not used, like ```_req```, ```_depot``` in this example:

    ``` rust
    #[fn_handler]
    async fn hello_world(res: &mut Response) {
        res.render_plain_text("Hello World");
    }
    ```

- Any type can be function handler's return value if it implements ```Writer```. For example &str implements ```Writer``` and it will render string as plain text:

    ```rust
    #[fn_handler]
    async fn hello_world(res: &mut Response) -> &'static str {// just return &str
        "Hello World"
    }
    ```

- The more common situation is we want to return a ```Result<T, E>``` to implify error handling. If ```T``` and ```E``` implements ```Writer```, ```Result<T, E>``` can be function handler's return type:
  
    ```rust
    #[fn_handler]
    async fn hello_world(res: &mut Response) -> Result<&'static str, ()> {// return Result
        Ok("Hello World")
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
    let server = Server::new(router);
    server.bind(([0, 0, 0, 0], 7878)).await;
}
```

### Middleware
There is no difference between Handler and Middleware, Middleware is just Handler.
### Tree-like routing system
Router supports nested, and you can add middleware in router. In this example, there are two routers, both of them has same path router ```"user"```, and both of them added to the same parent router, to do that because we want to add middleware to them and let them has different access control:

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
async fn index() -> &'static str {
    "Hello world!"
}
#[fn_handler]
async fn auth() -> &'static str {
    "user has authed\n\n"
}
#[fn_handler]
async fn list_users() -> &'static str {
    "list users"
}
#[fn_handler]
async fn show_user() -> &'static str {
    "show user"
}
#[fn_handler]
async fn create_user() -> &'static str {
    "user created"
}
#[fn_handler]
async fn update_user() -> &'static str {
    "user updated"
}
#[fn_handler]
async fn delete_user() -> &'static str {
    "user deleted"
}
```

### File upload
We can get file async by the function ```get_file``` in ```Request```:

```rust
#[fn_handler]
async fn upload(req: &mut Request, res: &mut Response) {
    let file = req.get_file("file").await;
    if let Some(file) = file {
        let dest = format!("temp/{}", file.filename().unwrap_or_else(|| "file".into()));
        if let Err(e) = std::fs::copy(&file.path, Path::new(&dest)) {
            res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
        } else {
            res.render_plain_text("Ok");
        }
    } else {
        res.set_status_code(StatusCode::BAD_REQUEST);
    }
}
```

Multiple files also very simple:

```rust
#[fn_handler]
async fn upload(req: &mut Request, res: &mut Response) {
    let files = req.get_files("files").await;
    if let Some(files) = files {
        let mut msgs = Vec::with_capacity(files.len());
        for file in files {
            let dest = format!("temp/{}", file.filename().unwrap_or_else(|| "file".into()));
            if let Err(e) = std::fs::copy(&file.path, Path::new(&dest)) {
                res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render_plain_text(&format!("file not found in request: {}", e.to_string()));
            } else {
                msgs.push(dest);
            }
        }
        res.render_plain_text(&format!("Files uploaded:\n\n{}", msgs.join("\n")));
    } else {
        res.set_status_code(StatusCode::BAD_REQUEST);
        res.render_plain_text("file not found in request");
    }
}
```

### More Examples
Your can find more examples in [examples](./examples/) folder:
- [basic_auth.rs](./examples/basic_auth.rs)
- [compression.rs](./examples/compression.rs)
- [file_list.rs](./examples/file_list.rs)
- [proxy.rs](./examples/proxy.rs)
- [remote_addr.rs](./examples/remote_addr.rs)
- [routing.rs](./examples/routing.rs)
- [sse_chat.rs](./examples/sse_chat.rs)
- [sse.rs](./examples/sse.rs)
- [tls.rs](./examples/tls.rs)
- [todos.rs](./examples/todos.rs)
- [unix_socket.rs](./examples/unix_socket.rs)
- [ws_chat.rs](./examples/ws_chat.rs)
- [ws.rs](./examples/ws.rs)

Some code and examples port from from [warp](https://github.com/seanmonstar/warp) and [multipart-async](https://github.com/abonander/multipart-async).

## ☕ Supporters

Salvo is an open source project. If you want to support Salvo, you can ☕ [**buy a coffee here**](https://www.buymeacoffee.com/chrislearn).

## ⚠️ License

Salvo is licensed under either of
* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))
* MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
