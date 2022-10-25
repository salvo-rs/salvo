use salvo::prelude::*;
use salvo_rate_limiter::{BasicQuota, FixedGuard, MemoryStore, RateLimiter, RemoteIpIssuer};

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let limiter = RateLimiter::new(
        FixedGuard::new(),
        MemoryStore::new(),
        RemoteIpIssuer,
        BasicQuota::per_second(1),
    );
    let router = Router::with_hoop(limiter).get(hello);
    Server::new(TcpListener::bind("127.0.0.1:7878")).await.serve(router).await;
}
