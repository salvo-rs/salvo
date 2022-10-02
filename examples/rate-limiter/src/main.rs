use std::time::Duration;

use salvo::prelude::*;
use salvo_rate_limiter::{real_ip_identifer, MemoryStore, RateLimiter, SlidingWindow};

#[handler]
async fn hello_world() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://127.0.0.1:7878");
    let limiter = RateLimiter::new(SlidingWindow::new(1, Duration::from_secs(5)), MemoryStore::new(), real_ip_identifer);
    let router = Router::with_hoop(limiter).get(hello_world);
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}

#[cfg(test)]
mod tests {
    use salvo::prelude::*;
    use salvo::test::{ResponseExt, TestClient};

    #[tokio::test]
    async fn test_hello_world() {
        let service = Service::new(super::route());

        async fn access(service: &Service, name: &str) -> String {
            TestClient::get(format!("http://127.0.0.1:7878/{}", name))
                .send(service)
                .await
                .take_string()
                .await
                .unwrap()
        }

        assert_eq!(access(&service, "hello1").await, "Hello World1");
        assert_eq!(access(&service, "hello2").await, "Hello World2");
        assert_eq!(access(&service, "hello3").await, "Hello World3");
    }
}
