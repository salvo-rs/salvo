use salvo::extra::compression;
use salvo::extra::serve::*;
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .push(Router::with_path("ws_chat").get(StaticFile::new("examples/ws_chat.rs")))
        .push(
            Router::new()
                .after(compression::deflate())
                .path("sse_chat")
                .get(StaticFile::new("examples/sse_chat.rs")),
        )
        .push(
            Router::new()
                .after(compression::brotli())
                .path("todos")
                .get(StaticFile::new("examples/todos.rs")),
        )
        .push(
            Router::new()
                .after(compression::gzip())
                .path("examples/<*path>")
                .get(StaticDir::new("examples/")),
        );
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}
