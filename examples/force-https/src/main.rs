use salvo::conn::rustls::{Keycert, RustlsConfig};
use salvo::prelude::*;

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello);
    let service = Service::new(router).hoop(ForceHttps::new().https_port(5443));

    let config = RustlsConfig::new(
        Keycert::new()
            .cert(include_bytes!("../certs/cert.pem").as_ref())
            .key(include_bytes!("../certs/key.pem").as_ref()),
    );
    let acceptor = TcpListener::new("0.0.0.0:5443")
        .rustls(config)
        .join(TcpListener::new("0.0.0.0:5800"))
        .bind()
        .await;
    Server::new(acceptor).serve(service).await;
}
