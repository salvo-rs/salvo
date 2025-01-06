use salvo::prelude::*;
use salvo::serve_static::StaticDir;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("{*path}").get(
        StaticDir::new([
            "static-dir-list/static/boy",
            "static-dir-list/static/girl",
            "static/boy",
            "static/girl",
        ])
        .include_dot_files(false)
        .defaults("index.html")
        .auto_list(true),
    );

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    Server::new(acceptor).serve(router).await;
}
