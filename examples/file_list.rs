use salvo_core::prelude::*;
use salvo_extra::serve::StaticDir;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new()
        .path("<**path>")
        .get(StaticDir::new(vec!["examples/static/body", "examples/static/girl"]));
    Server::new(router).bind(([0, 0, 0, 0], 7878)).await;
}
