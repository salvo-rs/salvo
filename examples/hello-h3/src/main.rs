use std::sync::Arc;

use rustls::{Certificate, PrivateKey};
use salvo::conn::rustls::{Keycert, RustlsConfig};
use salvo::prelude::*;

#[handler]
async fn hello(res: &mut Response) -> &'static str {
    res.add_header(
        "alt-svc",
        r#"h3-29=":8080"; ma=2592000,quic=":8080"; ma=2592000; v="46,43""#,
        true,
    )
    .unwrap();
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let cert = include_bytes!("../certs/cert.pem").to_vec();
    let key = include_bytes!("../certs/key.pem").to_vec();

    let router = Router::new().get(hello);
    let config = RustlsConfig::new(Keycert::new().with_cert(cert.as_slice()).with_key(key.as_slice()));
    let listener = RustlsListener::bind(config.clone(), "127.0.0.1:7878");

    let listener = QuicListener::bind(config, ("127.0.0.1", 7878)).join(listener);

    Server::new(listener).serve(router).await;
}
