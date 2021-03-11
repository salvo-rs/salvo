#![deny(warnings)]

use salvo_core::prelude::*;
use salvo_extra::compression;
use salvo_extra::serve::*;
use tracing_subscriber;
use tracing_subscriber::fmt::format::FmtSpan;

#[tokio::main]
async fn main() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "compression=debug,salvo=debug".to_owned());
    tracing_subscriber::fmt().with_env_filter(filter).with_span_events(FmtSpan::CLOSE).init();

    let router = Router::new().push(
        Router::new().path("ws_chat").get(StaticFile::new("examples/ws_chat.rs"))
    ).push(
        Router::new().after(compression::deflate()).path("sse_chat").get(StaticFile::new("examples/sse_chat.rs"))
    ).push(
        Router::new().after(compression::brotli()).path("todos").get(StaticFile::new("examples/todos.rs"))
    ).push(
        Router::new().after(compression::gzip()).path("examples/<*path>").get(StaticDir::new("examples/"))
    );
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}
