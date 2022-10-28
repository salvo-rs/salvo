# Caching Headers

提供对缓存头配置支持的中间件.

实际上实现内部包含了 `CachingHeaders`, `Modified`, `ETag` 三个 `Handler` 的实现, `CachingHeaders` 是后两者的组合. 正常情况下使用 `CachingHeaders`.

## 配置 Cargo.toml

```toml
salvo = { version = "*", features = ["caching-headers"] }
```

## 示例代码

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