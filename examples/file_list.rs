use salvo_core::routing::Router;
use salvo_core::Server;
use salvo_extra::serve::Static;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let router = Router::new().path("files/<*path>").get(Static::from("./static"));
    let server = Server::with_addr(router, "127.0.0.1:9688");
    server.serve().await?;
    Ok(())
}
