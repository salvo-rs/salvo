use salvo_core::routing::Router;
use salvo_core::Server;
use salvo_extra::serve::StaticDir;
use tracing_subscriber;
use tracing_subscriber::fmt::format::FmtSpan;

#[tokio::main]
async fn main() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "hello_world=debug,salvo=debug".to_owned());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(FmtSpan::CLOSE)
        .init();
    let router = Router::new()
        .path("<**path>")
        .get(StaticDir::new(vec!["examples/static/body", "examples/static/girl"]));
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}
