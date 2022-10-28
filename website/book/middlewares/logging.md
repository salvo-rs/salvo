# Logging

Middleware that provides basic Log functionality.

## Config Cargo.toml

```toml
salvo = { version = "*", features = ["logging"] }
```

## Sample Code

```rust
use salvo::logging::Logger;
use salvo::prelude::*;

#[handler]
async fn hello_world() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().hoop(Logger).get(hello_world);
    tracing::info!("Listening on http://127.0.0.1:7878");
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
}
```