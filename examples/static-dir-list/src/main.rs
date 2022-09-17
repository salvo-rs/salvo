use salvo::extra::serve_static::{StaticDir, StaticDirOptions};
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("<**path>").get(StaticDir::width_options(
        [
            "examples/static-dir-list/static/boy",
            "examples/static-dir-list/static/girl",
        ],
        StaticDirOptions::new().defaults("index.html").listing(true),
    ));
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
