//! An HTTP/3 request body reports the length announced by `content-length`.
#![cfg(all(feature = "quinn", feature = "server", feature = "server-handle"))]

use std::sync::Arc;
use std::time::Duration;

use bytes::{Buf, Bytes};
use http_body_util::BodyExt;
use hyper::body::Body;
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
    let mut body = req.take_body();
    let before = body.size_hint().exact();
    let mut received = Vec::new();
    let mut during = Vec::new();
    let mut error = None;
    while let Some(frame) = body.frame().await {
        match frame {
            Ok(frame) => {
                if let Ok(data) = frame.into_data() {
                    received.extend_from_slice(&data);
                    during.push(body.size_hint().exact());
                }
            }
            Err(failure) => {
                error = Some(failure.to_string());
                break;
            }
        }
    }
    res.render(Json(json!({
        "before": before,
        "during": during,
        "after": body.size_hint().exact(),
        "body": String::from_utf8(received).unwrap(),
        "error": error,
    })));
}

/// Sends the chunks as separate DATA frames and returns what the handler observed.
async fn observe(content_lengths: &[&str], chunks: &'static [&'static str]) -> Value {
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
        let mut request = http::Request::post("https://testserver.com/");
        for content_length in content_lengths {
            request = request.header("content-length", *content_length);
        }
        let mut stream = sender
            .send_request(request.body(()).unwrap())
            .await
            .unwrap();
        for chunk in chunks {
            stream
                .send_data(Bytes::from_static(chunk.as_bytes()))
                .await
                .unwrap();
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
    let (status, observation) = observation.expect("the request must finish");
    assert_eq!(status, StatusCode::OK);
    observation
}

#[tokio::test]
async fn announced_length_is_reported_and_counts_down() {
    let observed = observe(&["7"], &["payload"]).await;
    assert_eq!(observed["body"], "payload");
    assert_eq!(observed["before"], 7);
    assert_eq!(observed["after"], 0);
    // However the stream splits the data, the hint ends at zero once it all arrived.
    assert_eq!(
        observed["during"].as_array().unwrap().last(),
        Some(&json!(0))
    );
}

#[tokio::test]
async fn announced_empty_body_has_an_exact_length_of_zero() {
    let observed = observe(&["0"], &[]).await;
    assert_eq!(observed["before"], 0);
    assert_eq!(observed["after"], 0);
}

#[tokio::test]
async fn length_stays_unknown_without_a_usable_content_length() {
    for content_lengths in [&[][..], &["7", "7"][..], &["seven"][..], &["+7"][..]] {
        let observed = observe(content_lengths, &["payload"]).await;
        assert_eq!(observed["body"], "payload", "{content_lengths:?}");
        assert_eq!(observed["before"], Value::Null, "{content_lengths:?}");
        assert_eq!(observed["after"], Value::Null, "{content_lengths:?}");
        assert_eq!(observed["error"], Value::Null, "{content_lengths:?}");
    }
}

#[tokio::test]
async fn content_that_does_not_match_the_announced_length_is_an_error() {
    // The reported length is only exact if it is enforced.
    let longer = observe(&["3"], &["payload"]).await;
    assert_eq!(longer["before"], 3);
    assert_eq!(longer["body"], "");
    assert_eq!(
        longer["error"],
        "request content is longer than its content-length"
    );

    let shorter = observe(&["9"], &["payload"]).await;
    assert_eq!(shorter["before"], 9);
    assert_eq!(shorter["body"], "payload");
    assert_eq!(
        shorter["error"],
        "request content is shorter than its content-length"
    );
}
