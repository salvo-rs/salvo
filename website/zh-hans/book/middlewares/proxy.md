# Proxy

提供反向代理功能的中间件.

## 配置 Cargo.toml

```toml
salvo = { version = "*", features = ["proxy"] }
```

## 示例代码

```rust
use salvo::prelude::*;
use salvo_extra::proxy::ProxyHandler;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    
    let router = Router::new()
        .push(
            Router::new()
                .path("google/<**rest>")
                .handle(ProxyHandler::new(vec!["https://www.google.com".into()])),
        )
        .push(
            Router::new()
                .path("baidu/<**rest>")
                .handle(ProxyHandler::new(vec!["https://www.baidu.com".into()])),
        );
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(router).await;
}
```