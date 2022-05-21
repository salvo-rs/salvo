use salvo::extra::compression;
use salvo::extra::serve_static::*;
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .push(Router::with_path("ws_chat").get(FileHandler::new("examples/ws_chat.rs")))
        .push(
            Router::new()
                .hoop(compression::deflate())
                .path("sse_chat")
                .get(FileHandler::new("examples/sse_chat.rs")),
        )
        .push(
            Router::new()
                .hoop(compression::brotli())
                .path("todos")
                .get(FileHandler::new("examples/todos.rs")),
        )
        .push(
            Router::new()
                .hoop(compression::gzip())
                .path("examples/<*path>")
                .get(DirHandler::new("examples/")),
        );
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
