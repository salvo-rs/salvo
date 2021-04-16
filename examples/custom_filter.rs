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

    // only allow access from http://localhost:7878/, http://127.0.0.1:7878/ will get not found page.
    let router = Router::new()
        .filter_fn(|req, _| {
            let host = req.get_header::<String>("host").unwrap_or_default();
            host == "localhost:7878"
        })
        .get(hello_world);
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}
