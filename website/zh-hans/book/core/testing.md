# Testing

Salvo 提供的 test 模块, 可以帮助测试 Salvo 的项目.

[最新文档](https://docs.rs/salvo_core/latest/salvo_core/test/index.html)

**简单示例:**

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
    let acceptor = TcpListener::new("127.0.0.1:7878").bind().await; Server::new(acceptor).serve(route()).await;
}

fn route() -> Router {
    Router::new().get(hello_world)
}

#[cfg(test)]
mod tests {
    use salvo::prelude::*;
    use salvo::test::{ResponseExt, TestClient};

    #[tokio::test]
    async fn test_hello_world() {
        let service = Service::new(super::route());

        let content = TestClient::get(format!("http://127.0.0.1:7878/{}", name))
            .send(service)
            .await
            .take_string()
            .await
            .unwrap()
        assert_eq!(content, "Hello World");
    }
}
```