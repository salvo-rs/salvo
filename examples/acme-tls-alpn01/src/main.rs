use salvo::listener::AcmeListener;
use salvo::prelude::*;

#[handler]
async fn hello_world() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello_world);
    let listener = AcmeListener::builder()
        // .directory("letsencrypt", salvo::listener::acme::LETS_ENCRYPT_STAGING)
        .cache_path("acme/letsencrypt")
        .add_domain("acme-tls-alpn01.salvo.rs")
        .bind("0.0.0.0:443")
        .await;
    tracing::info!("Listening on https://0.0.0.0:443");
    Server::new(listener).serve(router).await;
}
