use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let mut router = Router::new().get(hello);
    let listener = TcpListener::new("0.0.0.0:443")
        .acme()
        // .directory("letsencrypt", salvo::listener::acme::LETS_ENCRYPT_STAGING)
        .cache_path("acme/letsencrypt")
        .add_domain("acme-http01.salvo.rs")
        .http01_challege(&mut router);
    Server::new(listener.join(TcpListener::new("0.0.0.0:80")).bind().await)
        .serve(router)
        .await;
}
