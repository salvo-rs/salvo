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
    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
