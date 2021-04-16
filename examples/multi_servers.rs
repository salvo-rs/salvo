use salvo::prelude::*;
use std::sync::Arc;
use tracing_subscriber;
use tracing_subscriber::fmt::format::FmtSpan;

#[fn_handler]
async fn hello_world1() -> &'static str {
    "Server1: Hello World"
}
#[fn_handler]
async fn hello_world2() -> &'static str {
    "Server2: Hello World"
}

#[tokio::main]
async fn main() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "salvo=debug".to_owned());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    let router1 = Router::new().get(hello_world1);
    let router2 = Router::new().get(hello_world2);

    tokio::join!(
        Server::new(router1).bind(([0, 0, 0, 0], 7878)),
        Server::new(router2).bind(([0, 0, 0, 0], 6868))
    );
}
