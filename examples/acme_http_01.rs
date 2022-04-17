use salvo::listener::AcmeListener;
use salvo::prelude::*;

#[fn_handler]
async fn hello_world() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    tracing_subscriber::fmt().init();

    let mut router = Router::new().get(hello_world);
    let listener = AcmeListener::builder()
        .add_domain("salvo.rs")
        .http01_challege(&router)
        .bind("0.0.0.0:443")
        .await?;
    tracing::info!("Listening on https://0.0.0.0:443");
    Server::new(listener.join(TcpListener::bind("0.0.0.0:80")))
        .serve(router)
        .await;
}
