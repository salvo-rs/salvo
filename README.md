# Salvo [![build status](https://github.com/kenorld/salvo/workflows/linux/badge.svg?branch=master&event=push)](https://github.com/kenorld/salvo/actions)[![build status](https://github.com/kenorld/salvo//workflows/macos/badge.svg?branch=master&event=push)](https://github.com/kenorld/salvo/actions)[![build status](https://github.com/kenorld/salvo/workflows/windows/badge.svg?branch=master&event=push)](https://github.com/kenorld/salvo/actions)[![codecov](https://codecov.io/gh/kenorld/salvo/branch/master/graph/badge.svg)](https://codecov.io/gh/kenorld/salvo) [![crates.io](https://img.shields.io/crates/v/salvo)](https://crates.io/crates/salvo)

Salvo is a simple web framework written by rust. It is simple to use it to build website, REST API.

## Features
  * Base on hyper, futures 0.3.
  * Easy to write router.

## Quick start
You can view samples [here](https://github.com/kenorld/salvo/tree/master/examples) or read docs [here](https://docs.rs/salvo/0.1.6/salvo/).

Create a new rust project:
```bash
cargo new salvo_taste --bin
```

Add this to `Cargo.toml`
```toml
[dependencies]
salvo = "0.2"
tokio = { version = "0.2", features = ["full"] }
```

Create a simple function handler in the main.rs file, we call it `hello_world`, this function just render plain text "Hello World".

```rust
use salvo::prelude::*;

#[fn_handler]
async fn hello_world(_conf: Arc<ServerConfig>, _req: &mut Request, _depot: &mut Depot, resp: &mut Response) {
    resp.render_plain_text("Hello World");
}
```

In the main function, we need to create a root Router first, and then create a server and call it's serve function:

```rust
use salvo::prelude::*;

#[fn_handler]
async fn hello_world(_conf: Arc<ServerConfig>, _req: &mut Request, _depot: &mut Depot, resp: &mut Response) {
    resp.render_plain_text("Hello World");
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