use rust_embed::RustEmbed;
use salvo::prelude::*;
use salvo::serve_static::static_embed;

#[derive(RustEmbed)]
#[folder = "static"]
struct Assets;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::with_path("<**path>").get(static_embed::<Assets>().with_fallback("index.html"));
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}
