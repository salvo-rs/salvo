use salvo::prelude::*;
use salvo::serve_static::StaticDir;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("<**path>").get(
        StaticDir::new([
            "examples/static-dir-list/static/boy",
            "examples/static-dir-list/static/girl",
        ])
        .with_defaults("index.html")
        .with_listing(true),
    );

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
