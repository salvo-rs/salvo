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
<h3>Salvo 是一个简单易用的 Rust Web 后端框架.</h3>
</div>

## 🎯 功能
  * 基于最新版本的 hyper, tokio;
  * 支持 Websocket;
  * 支持从多个本地目录映射成一个虚拟目录提供服务;
  * 中间件系统支持在句柄之前或者之后运行;
  * 简单易用的路由系统:
    - 路径参数和和支持正则表达式;
    - 树状路由系统;

## ⚡️ 快速开始
你可以从[这里](https://github.com/salvo-rs/salvo/tree/master/examples)查看实例代码， 或者从[这里](https://docs.rs/salvo/0.1.6/salvo/)查看文档。


创建一个全新的项目:
```bash
cargo new hello_salvo --bin
```

添加依赖项到 `Cargo.toml`
```toml
[dependencies]
salvo = "0.8"
tokio = { version = "1", features = ["full"] }
```

在 `main.rs` 中创建一个简单的函数句柄, 命名为`hello_world`, 这个函数只是简单地打印文本 "Hello World".

```rust
use salvo::prelude::*;

#[fn_handler]
async fn hello_world(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render_plain_text("Hello World");
}
```

在 ```main``` 函数中, 我们需要首先创建一个根路由, 然后创建一个 Server 并且调用它的 ```bind``` 函数:

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

### 树状路由系统

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
### 更多示例
您可以从 [examples](./examples/) 文件夹下查看更多示例代码:
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

## ☕ 支持

`Salvo`是一个开源项目，如果想支持本项目, 可以 ☕ [**在这里买一杯咖啡**](https://www.buymeacoffee.com/chrislearn). 
<p style="text-align: center;">
<img src="assets/alipay.png" alt="Alipay" width="320"/>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;<img src="assets/weixin.png" alt="Weixin" width="320"/>
</p>


## ⚠️ 开源协议

Salvo 项目采用 MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
