use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use bytes::{Bytes, BytesMut};
use futures::StreamExt;
use rustls::{Certificate, PrivateKey};
use structopt::StructOpt;
use tokio::{fs::File, io::AsyncReadExt};

use salvo::conn::rustls::{Keycert, RustlsConfig};
use salvo::prelude::*;

#[handler]
async fn hello(res: &mut Response) -> &'static str {
    res.add_header(
        "alt-svc",
        r#"h3-29=":7878"; ma=2592000,quic=":7878"; ma=2592000; v="46,43""#,
        true,
    );
    "Hello World"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let (cert, key) = build_certs();

    let router = Router::new().get(hello);
    let config = RustlsConfig::new(Keycert::new().with_cert(cert.as_slice()).with_key(key.as_slice()));
    let listener = RustlsListener::bind(config, "127.0.0.1:7878");
    
    let cert = Certificate(cert);
    let key = PrivateKey(key);
    let mut crypto = rustls::ServerConfig::builder()
        .with_safe_default_cipher_suites()
        .with_safe_default_kx_groups()
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key).unwrap();
    crypto.max_early_data_size = u32::MAX;
    crypto.alpn_protocols = vec![ALPN.into()];
    let server_config = salvo::conn::quic::ServerConfig::with_crypto(Arc::new(crypto));
    let listener = QuicListener::bind(("127.0.0.1", 7878), server_config).join(listener);

    Server::new(listener).serve(router).await;
}

static ALPN: &[u8] = b"h3";

pub fn build_certs() -> (Vec<u8>, Vec<u8>) {
    let cert = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    (cert.serialize_der().unwrap(), cert.serialize_private_key_der())
}
