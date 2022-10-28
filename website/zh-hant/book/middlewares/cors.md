# CORS

CORS 中間件可以用於 [跨域資源共享](https://developer.mozilla.org/zh-CN/docs/Web/HTTP/CORS).

由於瀏覽器會發 `Method::OPTIONS` 的請求, 所以需要增加對此類請求的處理. 可以對根 `Router` 添加 `empty_handler` 一勞永逸地處理這種情況.

## 配置 Cargo.toml

```toml
salvo = { version = "*", features = ["cors"] }
```

## 示例代碼

```rust
use salvo::prelude::*;
use salvo_extra::cors::Cors;

#[handler]
async fn hello() -> &'static str {
    "hello"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let cors_handler = Cors::builder()
        .with_allow_origin("https://salvo.rs")
        .with_allow_methods(vec!["GET", "POST", "DELETE"])
        .build();

    let router = Router::with_hoop(cors_handler).get(hello).options(empty_handler);
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
```