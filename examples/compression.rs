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
                .hoop(compression::deflate())
                .path("sse_chat")
                .get(StaticFile::new("examples/sse_chat.rs")),
        )
        .push(
            Router::new()
                .hoop(compression::brotli())
                .path("todos")
                .get(StaticFile::new("examples/todos.rs")),
        )
        .push(
            Router::new()
                .hoop(compression::gzip())
                .path("examples/<*path>")
                .get(StaticDir::new("examples/")),
        );
    Server::bind(&"127.0.0.1:7878".parse().unwrap())
        .serve(Service::new(router))
        .await
        .unwrap();
}
