# Testing

Salvo 提供的 test 模塊, 可以幫助測試 Salvo 的項目.

[最新文檔](https://docs.rs/salvo_core/latest/salvo_core/test/index.html)

**簡單示例:**

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
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(route()).await;
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