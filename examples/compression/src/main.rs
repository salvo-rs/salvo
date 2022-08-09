use salvo::extra::compression;
use salvo::extra::serve_static::*;
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let base_dir = std::env::current_exe()
        .unwrap()
        .join("../../../examples/compression/static")
        .canonicalize()
        .unwrap();
    println!("Base Dir: {:?}", base_dir);

    let router = Router::new()
        .push(Router::with_path("ws_chat").get(FileHandler::new(base_dir.join("ws_chat.txt"))))
        .push(
            Router::new()
                .hoop(compression::deflate())
                .path("sse_chat")
                .get(FileHandler::new(base_dir.join("sse_chat.txt"))),
        )
        .push(
            Router::new()
                .hoop(compression::brotli())
                .path("todos")
                .get(FileHandler::new(base_dir.join("todos.txt"))),
        )
        .push(
            Router::new()
                .hoop(compression::gzip())
                .path("<*path>")
                .get(DirHandler::new(base_dir)),
        );
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
