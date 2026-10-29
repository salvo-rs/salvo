//! A finite response body error must reset its stream without closing the connection.
#![cfg(all(feature = "quinn", feature = "server", feature = "server-handle"))]

use std::io;
use std::sync::Arc;
use std::time::Duration;

use bytes::{Buf, Bytes};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use salvo_core::conn::quinn::QuinnAcceptor;
use salvo_core::conn::rustls::{Keycert, RustlsConfig, default_crypto_provider};
use salvo_core::http::body::{BytesFrame, Frame};
use salvo_core::prelude::*;
use salvo_http3::error::{Code, StreamError};
use salvo_http3::ext::Protocol;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const DEADLINE: Duration = Duration::from_secs(5);

#[handler]
async fn responses(req: &mut Request, res: &mut Response) {
    if req.uri().path() == "/broken" {
        if req.method() == http::Method::CONNECT {
            res.status_code(StatusCode::FORBIDDEN);
        }
        res.stream(futures_util::stream::iter([
            Ok(BytesFrame::data("partial")),
            Err(io::Error::other("finite response body error")),
            Ok(BytesFrame::data("must not be sent")),
        ]));
    } else {
        let mut trailers = http::HeaderMap::new();
        trailers.insert("x-checksum", "complete".parse().unwrap());
        res.stream(futures_util::stream::iter([
            Ok::<_, io::Error>(BytesFrame::data("complete")),
            Ok(BytesFrame(Frame::trailers(trailers))),
        ]));
    }
}

async fn body_error_preserves_connection(webtransport: bool) {
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
    let router = Router::new()
        .push(Router::with_path("broken").goal(responses))
        .push(Router::with_path("complete").goal(responses));
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

        let mut request = http::Request::builder()
            .method(if webtransport { "CONNECT" } else { "GET" })
            .uri("https://testserver.com/broken")
            .body(())
            .unwrap();
        if webtransport {
            request.extensions_mut().insert(Protocol::WEB_TRANSPORT);
        }
        let mut broken = sender.send_request(request).await.unwrap();
        broken.finish().await.unwrap();
        // QUIC may deliver the reset before buffered response headers or data.
        let error = match broken.recv_response().await {
            Err(error) => error,
            Ok(_) => loop {
                match broken.recv_data().await {
                    Err(error) => break error,
                    Ok(Some(_)) => {}
                    Ok(None) => panic!("body error was reported as a successful response end"),
                }
            },
        };
        assert!(matches!(
            error,
            StreamError::RemoteTerminate {
                code: Code::H3_INTERNAL_ERROR,
                ..
            }
        ));

        let mut healthy = sender
            .send_request(
                http::Request::get("https://testserver.com/complete")
                    .body(())
                    .unwrap(),
            )
            .await
            .unwrap();
        healthy.finish().await.unwrap();
        assert_eq!(
            healthy.recv_response().await.unwrap().status(),
            StatusCode::OK
        );
        let mut body = Vec::new();
        while let Some(mut data) = healthy.recv_data().await.unwrap() {
            body.extend_from_slice(&data.copy_to_bytes(data.remaining()));
        }
        assert_eq!(body, b"complete");
        assert_eq!(
            healthy.recv_trailers().await.unwrap().unwrap()["x-checksum"],
            "complete"
        );
        assert!(connection.close_reason().is_none());
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
async fn h3_body_error_resets_only_its_response_stream() {
    body_error_preserves_connection(false).await;
}

#[tokio::test]
async fn rejected_webtransport_body_error_returns_connection() {
    body_error_preserves_connection(true).await;
}
