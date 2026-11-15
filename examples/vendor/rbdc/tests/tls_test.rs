//! End-to-end tests for the TLS wrapper using a local, in-process TLS server.
//!
//! The certificate/key pair in `tests/fixtures` is a throwaway self-signed
//! pair generated solely for these tests.

use rbdc::net::{CertificateInput, MaybeTlsStream};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::{PrivateKeyDer, pem::PemObject};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

const CA_PEM: &str = include_str!("fixtures/test_ca.pem");
const SERVER_CERT_PEM: &str = include_str!("fixtures/test_server_cert.pem");
const SERVER_KEY_PEM: &str = include_str!("fixtures/test_server_key.pem");

fn load_cert_and_key() -> (
    Vec<CertificateDer<'static>>,
    rustls::pki_types::PrivateKeyDer<'static>,
) {
    let certs = CertificateDer::pem_slice_iter(SERVER_CERT_PEM.as_bytes())
        .collect::<Result<_, _>>()
        .unwrap();
    let key = PrivateKeyDer::from_pem_slice(SERVER_KEY_PEM.as_bytes()).unwrap();
    (certs, key)
}

fn acceptor() -> TlsAcceptor {
    let (certs, key) = load_cert_and_key();
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    TlsAcceptor::from(Arc::new(config))
}

async fn spawn_server() -> (u16, tokio::task::JoinHandle<()>) {
    spawn_server_with(acceptor()).await
}

async fn spawn_server_with(acceptor: TlsAcceptor) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut tls = acceptor.accept(tcp).await.unwrap();
        let mut buf = [0u8; 4];
        tls.read_exact(&mut buf).await.unwrap();
        tls.write_all(b"pong").await.unwrap();
        tls.flush().await.unwrap();
        tls.shutdown().await.unwrap();
    });
    (port, handle)
}

async fn assert_echo(stream: &mut MaybeTlsStream<TcpStream>) {
    stream.write_all(b"ping").await.unwrap();
    stream.flush().await.unwrap();
    let mut buf = [0u8; 4];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"pong");
    stream.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_tls_upgrade_accept_invalid_certs() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (port, server) = spawn_server().await;
    let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let mut stream = MaybeTlsStream::Raw(tcp);

    stream.upgrade("localhost", true, false, None).await.unwrap();
    assert!(stream.is_tls());
    // Deref / DerefMut to the underlying stream for the TLS variant.
    let _ = &*stream;
    let _ = &mut *stream;
    assert_echo(&mut stream).await;

    server.await.unwrap();
}

#[tokio::test]
async fn test_tls_upgrade_with_custom_root_and_wrong_hostname() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (port, server) = spawn_server().await;
    let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let mut stream = MaybeTlsStream::Raw(tcp);

    let ca: CertificateInput = CA_PEM.to_string().into();
    // The certificate is trusted via the custom root, so only the hostname
    // mismatch is handled by NoHostnameTlsVerifier.
    stream
        .upgrade("wrong.example", false, true, Some(&ca))
        .await
        .unwrap();
    assert!(stream.is_tls());
    assert_echo(&mut stream).await;

    server.await.unwrap();
}

#[tokio::test]
async fn test_tls_upgrade_twice_is_idempotent() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (port, server) = spawn_server().await;
    let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let mut stream = MaybeTlsStream::Raw(tcp);

    stream.upgrade("localhost", true, false, None).await.unwrap();
    // Upgrading an already-TLS stream is a no-op that preserves the stream.
    stream.upgrade("localhost", true, false, None).await.unwrap();
    assert!(stream.is_tls());
    assert_echo(&mut stream).await;

    server.await.unwrap();
}

#[tokio::test]
async fn test_tls_upgrade_with_custom_root_valid_hostname() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    // Exercises the plain "trusted roots + no hostname override" branch of
    // `configure_tls_connector`.
    let (port, server) = spawn_server().await;
    let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let mut stream = MaybeTlsStream::Raw(tcp);

    let ca: CertificateInput = CA_PEM.to_string().into();
    stream
        .upgrade("localhost", false, false, Some(&ca))
        .await
        .unwrap();
    assert!(stream.is_tls());
    assert_echo(&mut stream).await;

    server.await.unwrap();
}

#[tokio::test]
async fn test_tls_upgrade_with_invalid_der_root_cert_errors() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    // The PEM block decodes, but its contents are not a valid X.509
    // certificate, so adding it to the root store fails.
    let (client, _server) = tokio::io::duplex(64);
    let mut stream = MaybeTlsStream::Raw(client);
    let ca: CertificateInput =
        "-----BEGIN CERTIFICATE-----\naGVsbG8=\n-----END CERTIFICATE-----"
            .to_string()
            .into();
    let err = stream
        .upgrade("example.com", false, false, Some(&ca))
        .await
        .unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[tokio::test]
async fn test_tls_upgrade_no_hostname_verifier_accepts_valid_cert() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    // With `accept_invalid_hostnames`, the inner verifier still runs and can
    // succeed when the certificate is trusted and the hostname matches.
    let (port, server) = spawn_server().await;
    let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let mut stream = MaybeTlsStream::Raw(tcp);

    let ca: CertificateInput = CA_PEM.to_string().into();
    stream
        .upgrade("localhost", false, true, Some(&ca))
        .await
        .unwrap();
    assert!(stream.is_tls());
    assert_echo(&mut stream).await;

    server.await.unwrap();
}

#[tokio::test]
async fn test_upgrade_from_upgrading_state_errors() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (_client, _server) = tokio::io::duplex(64);
    let mut stream: MaybeTlsStream<tokio::io::DuplexStream> = MaybeTlsStream::Upgrading;
    let err = stream
        .upgrade("localhost", true, false, None)
        .await
        .unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[test]
fn pem_key_loader_rejects_missing_and_malformed_keys() {
    for pem in ["", CA_PEM, "not a private key", "-----BEGIN PRIVATE KEY-----\n!invalid!\n-----END PRIVATE KEY-----"] {
        assert!(PrivateKeyDer::from_pem_slice(pem.as_bytes()).is_err());
    }
    assert!(PrivateKeyDer::from_pem_slice(SERVER_KEY_PEM.as_bytes()).is_ok());
}

#[tokio::test]
async fn explicit_root_rejects_missing_or_malformed_certificates() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    for pem in ["", "not a certificate", SERVER_KEY_PEM, "-----BEGIN CERTIFICATE-----\n!invalid!\n-----END CERTIFICATE-----"] {
        let (client, _server) = tokio::io::duplex(64);
        let mut stream = MaybeTlsStream::Raw(client);
        let ca: CertificateInput = pem.to_owned().into();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            stream.upgrade("localhost", false, false, Some(&ca)),
        ).await.expect("bad PEM must fail before starting a handshake");
        assert!(result.is_err(), "accepted invalid or empty certificate input");
    }
}
