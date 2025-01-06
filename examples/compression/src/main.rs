use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    println!("current_dir: {:?}", std::env::current_dir().unwrap());
    let base_dir = std::env::current_dir()
        .unwrap()
        .join("compression/static")
        .canonicalize()
        .unwrap();
    println!("Base Dir: {base_dir:?}");

    let router = Router::new()
        .push(
            Router::with_hoop(Compression::new().force_priority(true))
                .path("ws_chat")
                .get(StaticFile::new(base_dir.join("ws_chat.txt"))),
        )
        .push(
            Router::with_hoop(Compression::new().enable_brotli(CompressionLevel::Fastest))
                .path("sse_chat")
                .get(StaticFile::new(base_dir.join("sse_chat.txt"))),
        )
        .push(
            Router::with_hoop(Compression::new().enable_zstd(CompressionLevel::Fastest))
                .path("todos")
                .get(StaticFile::new(base_dir.join("todos.txt"))),
        )
        .push(
            Router::with_hoop(Compression::new().enable_gzip(CompressionLevel::Fastest))
                .path("{*path}")
                .get(StaticDir::new(base_dir)),
        );

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
