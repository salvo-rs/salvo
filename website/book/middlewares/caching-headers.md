# Caching Headers

Middleware that provides support for cache header configuration.

In fact, the implementation contains three `Handler` implementations of `CachingHeaders`, `Modified`, `ETag`, and `CachingHeaders` is a combination of the latter two. Normally, `CachingHeaders` is used.

## Config Cargo.toml

```toml
salvo = { version = "*", features = ["caching-headers"] }
```

## Sample Code

```rust
use salvo::prelude::*;

#[handler]
async fn hello_world() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://127.0.0.1:7878");
    // Compression must be before CachingHeader.
    let router = Router::with_hoop(CachingHeaders::new())
        .hoop(Compression::new().with_min_length(0))
        .get(hello_world);
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
}
```