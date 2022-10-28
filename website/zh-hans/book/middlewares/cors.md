# CORS

CORS 中间件可以用于 [跨域资源共享](https://developer.mozilla.org/zh-CN/docs/Web/HTTP/CORS).

由于浏览器会发 `Method::OPTIONS` 的请求, 所以需要增加对此类请求的处理. 可以对根 `Router` 添加 `empty_handler` 一劳永逸地处理这种情况.

## 配置 Cargo.toml

```toml
salvo = { version = "*", features = ["cors"] }
```

## 示例代码

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
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
}
```