use salvo::extra::serve_static::{StaticDirOptions, StaticDir};
use salvo::prelude::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("<**path>").get(StaticDir::width_options(
        ["examples/file-list/static/boy", "examples/file-list/static/girl"],
        StaticDirOptions {
            dot_files: false,
            listing: true,
            defaults: vec!["index.html".to_owned()],
        },
    ));
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
