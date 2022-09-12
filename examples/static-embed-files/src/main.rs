use rust_embed::RustEmbed;
use salvo::extra::serve_static::static_embed;
use salvo::prelude::*;

#[derive(RustEmbed)]
#[folder = "static"]
struct Assets;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("<**path>").get(static_embed::<Assets>());
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
