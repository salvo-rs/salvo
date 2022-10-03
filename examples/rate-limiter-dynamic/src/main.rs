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
    let limiter = RateLimiter::new(
        SlidingWindow::new(),
        MemoryStore::new(),
        real_ip_identifer,
        RateQuota::new(120, Duration::from_secs(60)),
    );
    let router = Router::with_hoop(limiter).get(hello_world);
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
