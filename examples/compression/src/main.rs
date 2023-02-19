use salvo::compression::{Compression, CompressionAlgo};
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let base_dir = std::env::current_exe()
        .unwrap()
        .join("../../../examples/compression/static")
        .canonicalize()
        .unwrap();
    println!("Base Dir: {base_dir:?}");

    let router = Router::new()
        .push(
            Router::with_hoop(Compression::new().with_force_priority(true))
                .path("ws_chat")
                .get(StaticFile::new(base_dir.join("ws_chat.txt"))),
        )
        .push(
            Router::with_hoop(Compression::new().with_algos(&[CompressionAlgo::Brotli]))
                .path("sse_chat")
                .get(StaticFile::new(base_dir.join("sse_chat.txt"))),
        )
        .push(
            Router::with_hoop(Compression::new().with_algos(&[CompressionAlgo::Deflate]))
                .path("todos")
                .get(StaticFile::new(base_dir.join("todos.txt"))),
        )
        .push(
            Router::with_hoop(Compression::new().with_algos(&[CompressionAlgo::Gzip]))
                .path("<*path>")
                .get(StaticDir::new(base_dir)),
        );

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
