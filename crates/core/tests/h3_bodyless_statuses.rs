//! HTTP/3 obeys status codes that forbid response content.
#![cfg(all(feature = "quinn", feature = "server", feature = "server-handle"))]

use std::sync::Arc;
use std::time::Duration;

use bytes::{Buf, Bytes};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use salvo_core::conn::quinn::QuinnAcceptor;
use salvo_core::conn::rustls::{Keycert, RustlsConfig, default_crypto_provider};
use salvo_core::prelude::*;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const DEADLINE: Duration = Duration::from_secs(5);

#[handler]
async fn responses(req: &mut Request, res: &mut Response) {
    res.status_code(if req.uri().path() == "/status204" {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_MODIFIED
    });
    res.render("unexpected-body");
}

async fn bodyless_statuses_send_no_data() {
    let config = RustlsConfig::new(
        Keycert::new()
            .cert(include_bytes!("../certs/cert.pem").as_slice())
            .key(include_bytes!("../certs/key.pem").as_slice()),
    )
    .build_quinn_config()
    .unwrap();
    let endpoint = quinn::Endpoint::server(config, "127.0.0.1:0".parse().unwrap()).unwrap();
    let address = endpoint.local_addr().unwrap();
    let server = Server::new(QuinnAcceptor::new(
        endpoint,
        address,
        CancellationToken::new(),
    ));
    let handle = server.handle();
    let router = Router::new().push(Router::with_path("{**rest}").goal(responses));
    let server_task = tokio::spawn(server.try_serve(router));

    let result = timeout(DEADLINE, async {
        let mut roots = rustls::RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(include_bytes!("../certs/chain.pem")) {
            roots.add(cert.unwrap()).unwrap();
        }
        let mut tls = rustls::ClientConfig::builder_with_provider(default_crypto_provider())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        tls.alpn_protocols = vec![b"h3".to_vec()];
        let mut client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        client.set_default_client_config(quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap(),
        )));
        let connection = client
            .connect(address, "testserver.com")
            .unwrap()
            .await
            .unwrap();
        let (mut driver, mut sender) = salvo_http3::client::builder()
            .enable_extended_connect(true)
            .send_grease(false)
            .build::<_, _, Bytes>(salvo_http3::quinn::Connection::new(connection.clone()))
            .await
            .unwrap();
        let driver_task = tokio::spawn(async move { driver.wait_idle().await });

        for status in [204, 304] {
            let mut stream = sender
                .send_request(
                    http::Request::get(format!("https://testserver.com/status{status}"))
                        .body(())
                        .unwrap(),
                )
                .await
                .unwrap();
            stream.finish().await.unwrap();
            assert_eq!(
                stream.recv_response().await.unwrap().status().as_u16(),
                status
            );
            let mut body = Vec::new();
            while let Some(mut data) = stream.recv_data().await.unwrap() {
                body.extend_from_slice(&data.copy_to_bytes(data.remaining()));
            }
            println!("status {status} body {:?}", body);
            assert!(
                body.is_empty(),
                "body forbidden for status {status}: {body:?}"
            );
        }
        connection.close(0u32.into(), b"test complete");
        driver_task.abort();
    })
    .await;

    handle.stop_forceful();
    timeout(DEADLINE, server_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    result.expect("response error handling must finish");
}

#[tokio::test]
async fn h3_bodyless_statuses_have_no_data() {
    bodyless_statuses_send_no_data().await;
}
