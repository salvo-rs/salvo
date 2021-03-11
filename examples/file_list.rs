use salvo_core::routing::Router;
use salvo_core::Server;
use salvo_extra::serve::StaticDir;

#[tokio::main]
async fn main() {
    let router = Router::new().path("files/<*path>").get(StaticDir::new(vec!["examples/static/body", "examples/static/girl"]));
    Server::new(router).bind(([0, 0, 0, 0], 9688)).await;
}
