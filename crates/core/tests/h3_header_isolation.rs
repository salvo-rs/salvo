//! Incomplete request headers must not block independent HTTP/3 streams.
#![cfg(all(feature = "quinn", feature = "server", feature = "server-handle"))]

use std::sync::Arc;
use std::time::Duration;

use bytes::Buf;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use salvo_core::conn::quinn::QuinnAcceptor;
use salvo_core::conn::rustls::{Keycert, RustlsConfig, default_crypto_provider};
use salvo_core::prelude::*;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const DEADLINE: Duration = Duration::from_secs(5);

#[tokio::test]
async fn incomplete_h3_headers_do_not_block_a_sibling_request() {
    #[handler]
    async fn endpoint() -> &'static str {
        "done"
    }

    let server_config = RustlsConfig::new(
        Keycert::new()
            .cert(include_bytes!("../certs/cert.pem").as_slice())
            .key(include_bytes!("../certs/key.pem").as_slice()),
    )
    .build_quinn_config()
    .unwrap();
    let endpoint_quic =
        quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
    let address = endpoint_quic.local_addr().unwrap();
    let server = Server::new(QuinnAcceptor::new(
        endpoint_quic,
        address,
        CancellationToken::new(),
    ));
    let handle = server.handle();
    let server_task = tokio::spawn(server.try_serve(Router::new().get(endpoint)));

    let mut roots = rustls::RootCertStore::empty();
    for certificate in CertificateDer::pem_slice_iter(include_bytes!("../certs/chain.pem")) {
        roots.add(certificate.unwrap()).unwrap();
    }
    let mut tls = rustls::ClientConfig::builder_with_provider(default_crypto_provider())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls.alpn_protocols = vec![b"h3".to_vec()];
    let config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap(),
    ));
    let mut client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    client.set_default_client_config(config);
    let connection = timeout(DEADLINE, client.connect(address, "testserver.com").unwrap())
        .await
        .unwrap()
        .unwrap();
    let (mut driver, mut sender) = timeout(
        DEADLINE,
        salvo_http3::client::new(salvo_http3::quinn::Connection::new(connection.clone())),
    )
    .await
    .unwrap()
    .unwrap();
    let driver_task = tokio::spawn(async move { driver.wait_idle().await });

    // Open the lower-numbered request stream first and withhold its HEADERS
    // payload. QUIC accepts client-initiated streams in stream-ID order.
    let (mut incomplete, _receive) = connection.open_bi().await.unwrap();
    incomplete.write_all(&[0x01, 0x01]).await.unwrap();
    let mut sibling = sender
        .send_request(
            http::Request::get("https://testserver.com/")
                .body(())
                .unwrap(),
        )
        .await
        .unwrap();
    sibling.finish().await.unwrap();
    let response = timeout(DEADLINE, async {
        let response = sibling.recv_response().await.unwrap();
        let mut body = Vec::new();
        while let Some(mut data) = sibling.recv_data().await.unwrap() {
            body.extend_from_slice(&data.copy_to_bytes(data.remaining()));
        }
        (response.status(), body)
    })
    .await;

    // Always stop the listener, including when the regression assertion fails.
    handle.stop_forceful();
    timeout(DEADLINE, server_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    connection.close(0u32.into(), b"test complete");
    driver_task.abort();
    let (status, body) = response.expect("a stalled header stream must not block its sibling");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"done");
}
