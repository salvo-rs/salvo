use salvo::conn::openssl::{Keycert, OpensslConfig};
use salvo::prelude::*;

#[handler]
async fn hello(res: &mut Response) {
    res.render(Text::Plain("Hello World"));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello);
    let config = OpensslConfig::new(
        Keycert::new()
            .with_cert(include_bytes!("../certs/cert.pem").as_ref())
            .with_key(include_bytes!("../certs/key.pem").as_ref()),
    );
    let listener = OpensslListener::bind(config, "127.0.0.1:7878");
    Server::new(listener).await.serve(router).await;
}
