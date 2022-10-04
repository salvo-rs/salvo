use std::time::Duration;

use salvo::prelude::*;
use salvo_rate_limiter::{MemoryStore, RateLimiter, RealIpIssuer, CelledQuota, SlidingGuard};

#[handler]
async fn hello_world() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    tracing::info!("Listening on http://127.0.0.1:7878");
    let limiter = RateLimiter::new(
        SlidingGuard::new(),
        MemoryStore::new(),
        RealIpIssuer,
        CelledQuota::new(1, Duration::from_secs(3), 2),
    );
    let router = Router::with_hoop(limiter).get(hello_world);
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
