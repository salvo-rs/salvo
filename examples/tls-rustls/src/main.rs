use salvo::conn::rustls::{Keycert, RustlsConfig};
use salvo::prelude::*;

#[handler]
async fn hello(res: &mut Response) {
    res.render(Text::Plain("Hello World"));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello);
    let config = RustlsConfig::new(
        Keycert::new()
            .cert(include_bytes!("../certs/cert.pem").as_ref())
            .key(include_bytes!("../certs/key.pem").as_ref()),
    );
    let acceptor = TcpListener::new("0.0.0.0:5800").rustls(config).bind().await;
    Server::new(acceptor).serve(router).await;
}
