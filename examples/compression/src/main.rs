use salvo::prelude::*;

#[tokio::main]
async fn main() {
    // Initialize logging system
    tracing_subscriber::fmt().init();

    // Print current working directory for debugging
    println!("current_dir: {:?}", std::env::current_dir().unwrap());

    // Set up base directory for static files
    let base_dir = std::env::current_dir()
        .unwrap()
        .join("compression/static")
        .canonicalize()
        .unwrap();
    println!("Base Dir: {base_dir:?}");

    // Configure router with different compression settings for different paths
    let router = Router::new()
        // WebSocket chat with forced compression priority
        .push(
            Router::with_hoop(Compression::new().force_priority(true))
                .path("ws_chat")
                .get(StaticFile::new(base_dir.join("ws_chat.txt"))),
        )
        // SSE chat with Brotli compression
        .push(
            Router::with_hoop(Compression::new().enable_brotli(CompressionLevel::Fastest))
                .path("sse_chat")
                .get(StaticFile::new(base_dir.join("sse_chat.txt"))),
        )
        // Todos with Zstd compression
        .push(
            Router::with_hoop(Compression::new().enable_zstd(CompressionLevel::Fastest))
                .path("todos")
                .get(StaticFile::new(base_dir.join("todos.txt"))),
        )
        // All other paths with Gzip compression
        .push(
            Router::with_hoop(Compression::new().enable_gzip(CompressionLevel::Fastest))
                .path("{*path}")
                .get(StaticDir::new(base_dir)),
        );

    // Bind server to port 5800 and start serving
    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
