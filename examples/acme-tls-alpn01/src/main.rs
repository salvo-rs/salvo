use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello);
    let acceptor = TcpListener::new("0.0.0.0:443")
        .acme()
        // .cache_path("temp/letsencrypt")
        .add_domain("test.salvo.rs") // Replace this domain name with your own.
        .bind()
        .await;
    Server::new(acceptor).serve(router).await;
}
