# Caching Headers

提供對緩存頭配置支持的中間件.

實際上實現內部包含了 `CachingHeaders`, `Modified`, `ETag` 三個 `Handler` 的實現, `CachingHeaders` 是後兩者的組合. 正常情況下使用 `CachingHeaders`.

## 配置 Cargo.toml

```toml
salvo = { version = "*", features = ["caching-headers"] }
```

## 示例代碼

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
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
```