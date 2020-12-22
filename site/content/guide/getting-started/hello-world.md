+++
title = "Hello World"
weight = 1
+++


Create a new rust project:
```bash
cargo new salvo_taste --bin
```

Add this to `Cargo.toml`
```toml
[dependencies]
salvo = "0.3"
tokio = { version = "0.3", features = ["full"] }
```

Create a simple function handler in the main.rs file, we call it `hello_world`, this function just render plain text "Hello World".

```rust
use salvo::prelude::*;

#[fn_handler]
async fn hello_world(_conf: Arc<ServerConfig>, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
    res.render_plain_text("Hello World");
}
```

In the main function, we need to create a root Router first, and then create a server and call it's serve function:

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