use std::sync::Arc;

use rustls::{Certificate, PrivateKey};
use salvo::conn::rustls::{Keycert, RustlsConfig};
use salvo::prelude::*;

#[handler]
async fn hello(res: &mut Response) -> &'static str {
    res.add_header(
        "alt-svc",
        r#"h3-29=":7878"; ma=2592000,quic=":7878"; ma=2592000; v="46,43""#,
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
    let listener = RustlsListener::bind(config, "127.0.0.1:7878");

    let cert = rustls_pemfile::certs(&mut &*cert)
        .unwrap()
        .into_iter()
        .map(Certificate)
        .collect();
    let key = rustls_pemfile::pkcs8_private_keys(&mut &*key).unwrap().remove(0);
    let key = PrivateKey(key);
    let mut crypto = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(cert, key)
        .unwrap();
    crypto.max_early_data_size = u32::MAX;
    crypto.alpn_protocols = vec![b"h3-29".to_vec(), b"h3-28".to_vec(), b"h3-27".to_vec()];
    let server_config = salvo::conn::quic::ServerConfig::with_crypto(Arc::new(crypto));

    let listener = QuicListener::bind(("127.0.0.1", 7878), server_config).join(listener);

    Server::new(listener).serve(router).await;
}
