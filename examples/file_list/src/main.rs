use salvo_core::routing::Router;
use salvo_core::Server;
use salvo_extra::serve::Static;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut router = Router::new("/<*path>");
    router.get(Static::from("./static"));
    println!("{:#?}", &router);
    let server = Server::with_addr(router, "127.0.0.1:9688");
    server.serve().await?;
    Ok(())
}
