//! ACME listeners preserve negotiated TLS protocol state before HTTP detection.
#![allow(missing_docs, clippy::unwrap_used)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use http_body_util::{BodyExt, Empty};
use rcgen::{CertificateParams, CertificateSigningRequestParams, CertifiedIssuer, IsCa, KeyPair};
use salvo_acme::{AcmeListener, CertIssuer, IssuedCertificate, OcspConfig};
use salvo_core::conn::{Acceptor, Listener};
use salvo_core::prelude::*;
use salvo_core::rt::tokio::{TokioExecutor, TokioIo};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};

struct LocalIssuer(CertifiedIssuer<'static, KeyPair>);

#[salvo_core::async_trait]
impl CertIssuer for LocalIssuer {
    async fn issue(
        &self,
        csr: &[u8],
        _: &[String],
    ) -> salvo_acme::certon::Result<IssuedCertificate> {
        let request = CertificateSigningRequestParams::from_der(&csr.to_vec().into()).unwrap();
        let certificate = request.signed_by(&self.0).unwrap();
        Ok(IssuedCertificate {
            certificate_pem: format!("{}{}", certificate.pem(), self.0.pem()).into_bytes(),
            private_key_pem: Vec::new(),
            metadata: serde_json::json!({}),
        })
    }

    fn issuer_key(&self) -> String {
        "local-protocol-issuer".into()
    }
}

#[handler]
async fn hello(req: &mut Request) -> String {
    let protocol = req
        .tls_info()
        .map(|tls| tls.alpn_protocol().unwrap_or(b"none"))
        .unwrap_or(b"missing");
    format!("{:?} {}", req.version(), String::from_utf8_lossy(protocol))
}

async fn server() -> (SocketAddr, RootCertStore, tokio::task::JoinHandle<()>) {
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let ca = CertifiedIssuer::self_signed(params, KeyPair::generate().unwrap()).unwrap();
    let mut roots = RootCertStore::empty();
    roots.add(ca.der().clone()).unwrap();
    let acceptor = TcpListener::new("127.0.0.1:0")
        .acme()
        .directory("local", "http://127.0.0.1:1/directory")
        .add_domain("protocol.example")
        .add_issuer(Arc::new(LocalIssuer(ca)))
        .ocsp(OcspConfig {
            disable_stapling: true,
            ..Default::default()
        })
        .bind()
        .await;
    let address = acceptor.holdings()[0]
        .local_addr
        .clone()
        .into_std()
        .unwrap();
    let task = tokio::spawn(Server::new(acceptor).serve(Router::new().goal(hello)));
    (address, roots, task)
}

fn client(roots: RootCertStore, protocol: Option<&[u8]>) -> TlsConnector {
    let mut config =
        ClientConfig::builder_with_provider(salvo_core::conn::rustls::default_crypto_provider())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
    config.alpn_protocols = protocol.into_iter().map(<[u8]>::to_vec).collect();
    TlsConnector::from(Arc::new(config))
}

#[tokio::test]
async fn acme_serves_real_http2_without_enabling_cleartext_http2() {
    let (address, roots, server) = server().await;
    let tls = client(roots, Some(b"h2"))
        .connect(
            ServerName::try_from("protocol.example").unwrap(),
            tokio::net::TcpStream::connect(address).await.unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tls.get_ref().1.alpn_protocol(), Some(b"h2".as_slice()));
    let (mut sender, connection) = hyper::client::conn::http2::handshake::<
        _,
        _,
        Empty<hyper::body::Bytes>,
    >(TokioExecutor::new(), TokioIo::new(tls))
    .await
    .unwrap();
    let connection = tokio::spawn(connection);
    let response = tokio::time::timeout(
        Duration::from_secs(3),
        sender.send_request(
            hyper::Request::builder()
                .uri("https://protocol.example/")
                .body(Empty::new())
                .unwrap(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        "HTTP/2.0 h2"
    );
    drop(sender);
    connection.abort();
    server.abort();
}

#[tokio::test]
async fn acme_rejects_prefaces_that_disagree_with_alpn_even_when_h2c_is_enabled() {
    let (address, roots, server) = server().await;
    for (protocol, preface) in [
        (
            Some(b"h2".as_slice()),
            b"GET / HTTP/1.1\r\nHost: protocol.example\r\nConnection: close\r\n\r\n".as_slice(),
        ),
        (
            Some(b"http/1.1".as_slice()),
            b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n".as_slice(),
        ),
        (None, b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n".as_slice()),
    ] {
        let mut tls = client(roots.clone(), protocol)
            .connect(
                ServerName::try_from("protocol.example").unwrap(),
                tokio::net::TcpStream::connect(address).await.unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(tls.get_ref().1.alpn_protocol(), protocol);
        tls.write_all(preface).await.unwrap();
        let mut response = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(3), tls.read_to_end(&mut response))
            .await
            .expect("mismatched ALPN must close the connection");
        assert!(
            response.is_empty(),
            "unexpected HTTP response for {protocol:?}: {response:?}"
        );
    }
    server.abort();
}

#[tokio::test]
async fn acme_serves_http1_with_http1_or_no_alpn() {
    let (address, roots, server) = server().await;
    for (protocol, expected) in [
        (Some(b"http/1.1".as_slice()), "HTTP/1.1 http/1.1"),
        (None, "HTTP/1.1 none"),
    ] {
        let mut tls = client(roots.clone(), protocol)
            .connect(
                ServerName::try_from("protocol.example").unwrap(),
                tokio::net::TcpStream::connect(address).await.unwrap(),
            )
            .await
            .unwrap();
        tls.write_all(b"GET / HTTP/1.1\r\nHost: protocol.example\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(3), tls.read_to_end(&mut response))
            .await
            .unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.ends_with(expected), "{response}");
    }
    server.abort();
}
