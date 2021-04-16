use salvo::prelude::*;
use tracing_subscriber;
use tracing_subscriber::fmt::format::FmtSpan;

#[fn_handler]
async fn hello_world() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "salvo=debug".to_owned());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    let router = Router::new().filter_fn(|req, _|{
        println!("{:?}", req.uri());
        true
    }).get(hello_world);
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}
