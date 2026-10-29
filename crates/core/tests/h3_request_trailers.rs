//! Request body consumers must see HTTP/3 trailers as body frames.
#![cfg(all(feature = "quinn", feature = "server", feature = "server-handle"))]

use std::sync::Arc;
use std::time::Duration;

use bytes::{Buf, Bytes};
use http_body_util::BodyExt;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use salvo_core::conn::quinn::QuinnAcceptor;
use salvo_core::conn::rustls::{Keycert, RustlsConfig, default_crypto_provider};
use salvo_core::prelude::*;
use serde_json::{Value, json};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const DEADLINE: Duration = Duration::from_secs(5);

#[handler]
async fn inspect_body(req: &mut Request, res: &mut Response) {
    let collected = req.take_body().collect().await.unwrap();
    let checksums: Vec<_> = collected
        .trailers()
        .into_iter()
        .flat_map(|headers| headers.get_all("x-checksum"))
        .map(|value| value.to_str().unwrap().to_owned())
        .collect();
    let body = collected.to_bytes();
    res.render(Json(json!({
        "body": String::from_utf8(body.to_vec()).unwrap(),
        "checksums": checksums,
    })));
}

async fn request_body_observation(payload: &'static str, with_trailers: bool) -> Value {
    let server_config = RustlsConfig::new(
        Keycert::new()
            .cert(include_bytes!("../certs/cert.pem").as_slice())
            .key(include_bytes!("../certs/key.pem").as_slice()),
    )
    .build_quinn_config()
    .unwrap();
    let endpoint = quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
    let address = endpoint.local_addr().unwrap();
    let server = Server::new(QuinnAcceptor::new(
        endpoint,
        address,
        CancellationToken::new(),
    ));
    let handle = server.handle();
    let server_task = tokio::spawn(server.try_serve(Router::new().post(inspect_body)));

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
    let mut client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    client.set_default_client_config(quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap(),
    )));
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

    let observation = timeout(DEADLINE, async {
        let mut stream = sender
            .send_request(
                http::Request::post("https://testserver.com/")
                    .body(())
                    .unwrap(),
            )
            .await
            .unwrap();
        if !payload.is_empty() {
            stream
                .send_data(Bytes::from_static(payload.as_bytes()))
                .await
                .unwrap();
        }
        if with_trailers {
            let mut trailers = http::HeaderMap::new();
            trailers.append("x-checksum", "first".parse().unwrap());
            trailers.append("x-checksum", "second".parse().unwrap());
            stream.send_trailers(trailers).await.unwrap();
        }
        stream.finish().await.unwrap();
        let response = stream.recv_response().await.unwrap();
        let mut body = Vec::new();
        while let Some(mut chunk) = stream.recv_data().await.unwrap() {
            body.extend_from_slice(&chunk.copy_to_bytes(chunk.remaining()));
        }
        (
            response.status(),
            serde_json::from_slice::<Value>(&body).unwrap(),
        )
    })
    .await;

    handle.stop_forceful();
    timeout(DEADLINE, server_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    connection.close(0u32.into(), b"test complete");
    driver_task.abort();
    let (status, observation) = observation.expect("request body collection must finish");
    assert_eq!(status, StatusCode::OK);
    observation
}

#[tokio::test]
async fn h3_body_collection_preserves_data_and_repeated_trailers() {
    assert_eq!(
        request_body_observation("payload", true).await,
        json!({"body": "payload", "checksums": ["first", "second"]})
    );
}

#[tokio::test]
async fn h3_body_collection_preserves_trailers_without_data() {
    assert_eq!(
        request_body_observation("", true).await,
        json!({"body": "", "checksums": ["first", "second"]})
    );
}

#[tokio::test]
async fn h3_body_without_trailers_still_finishes() {
    assert_eq!(
        request_body_observation("payload", false).await,
        json!({"body": "payload", "checksums": []})
    );
}
