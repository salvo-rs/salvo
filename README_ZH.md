<div align="center">
<h1>Salvo</h1>
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
<h3>Salvo 是一个简单的 Rust Web 框架.</h3>
</div>

## 功能
  * 基于 hyper, tokio.
  * 树状路由系统.

## 快速开始
你可以从[这里](https://github.com/kenorld/salvo/tree/master/examples)查看实例代码， 或者从[这里](https://docs.rs/salvo/0.1.6/salvo/)查看文档。


创建一个全新的项目:
```bash
cargo new hello_salvo --bin
```

添加依赖项到 `Cargo.toml`
```toml
[dependencies]
salvo = "0.4"
tokio = { version = "1.0", features = ["full"] }
```

在 `main.rs` 中创建一个简单的函数句柄, 命名为`hello_world`, 这个函数只是简单地打印文本 "Hello World".

```rust
use salvo::prelude::*;

#[fn_handler]
async fn hello_world(_req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render_plain_text("Hello World");
}
```

在 main 函数中, 我们需要首先创建一个根路由, 然后创建一个 Server 并且调用它的 server 函数:

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

## License

Salvo is licensed under MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)