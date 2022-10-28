# Proxy

提供反向代理功能的中間件.

## 配置 Cargo.toml

```toml
salvo = { version = "*", features = ["proxy"] }
```

## 示例代碼

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
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
```