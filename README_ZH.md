Salvo 是一个简单的 Rust Web 框架，用于构建普通的 Web 网站，REST API 等。


## 快速开始
你可以从[这里](https://github.com/kenorld/salvo/tree/master/examples)查看实例代码， 或者从[这里](https://docs.rs/salvo/0.1.6/salvo/)查看文档。


创建一个全新的项目:
```bash
cargo new salvo_taste --bin
```

添加依赖项到 `Cargo.toml`
```toml
[dependencies]
salvo = "0.2"
tokio = { version = "0.3", features = ["full"] }
```

在 `main.rs` 中创建一个简单的函数句柄, 命名为`hello_world`, 这个函数只是简单地打印文本 "Hello World".

```rust
use salvo::prelude::*;

#[fn_handler]
async fn hello_world(_conf: Arc<ServerConfig>, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render_plain_text("Hello World");
}
```

在 main 函数中, 我们需要首先创建一个根路由, 然后创建一个 Server 并且调用它的 server 函数:

```rust
use salvo::prelude::*;

#[fn_handler]
async fn hello_world(_conf: Arc<ServerConfig>, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render_plain_text("Hello World");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut router = Router::new("/");
    router.get(hello_world);
    let server = Server::new(router);
    server.serve().await?;
    Ok(())
}
```

## License

Salvo is licensed under MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)