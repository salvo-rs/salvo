//! HTTP/3 request tasks must remain owned by their connection.

#![cfg(all(feature = "quinn", feature = "server", feature = "server-handle"))]

use std::sync::Arc;
use std::time::Duration;

use bytes::Buf;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use salvo_core::conn::quinn::QuinnAcceptor;
use salvo_core::conn::rustls::{Keycert, RustlsConfig, default_crypto_provider};
use salvo_core::prelude::*;
use tokio::sync::{Notify, mpsc};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const DEADLINE: Duration = Duration::from_secs(5);

struct OnDrop(mpsc::UnboundedSender<()>);

impl Drop for OnDrop {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

struct Waiting {
    entered: mpsc::UnboundedSender<()>,
    dropped: mpsc::UnboundedSender<()>,
    release: Arc<Notify>,
    abort: bool,
}

#[salvo_core::async_trait]
impl Handler for Waiting {
    async fn handle(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let _on_drop = OnDrop(self.dropped.clone());
        self.entered.send(()).unwrap();
        self.release.notified().await;
        if self.abort {
            if req.uri().query() == Some("abort=1") {
                ctrl.conn().abort();
            } else {
                std::future::pending::<()>().await;
            }
        } else {
            res.render("done");
        }
    }
}

#[derive(Clone, Copy)]
enum Stop {
    Forceful,
    Disconnect,
    GracefulTimeout,
    Graceful,
    HandlerAbort,
}

async fn requests_are_released(stop: Stop) {
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
    let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
    let (dropped_tx, mut dropped_rx) = mpsc::unbounded_channel();
    let release = Arc::new(Notify::new());
    let mut server_task = tokio::spawn(server.try_serve(Router::new().get(Waiting {
        entered: entered_tx,
        dropped: dropped_tx,
        release: release.clone(),
        abort: matches!(stop, Stop::HandlerAbort),
    })));

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
    let mut streams = Vec::new();
    for index in 0..2 {
        let path = if index == 0 { "/?abort=1" } else { "/" };
        let mut stream = timeout(
            DEADLINE,
            sender.send_request(
                http::Request::get(format!("https://testserver.com{path}"))
                    .body(())
                    .unwrap(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        stream.finish().await.unwrap();
        streams.push(stream);
    }
    for _ in 0..2 {
        assert_eq!(
            timeout(DEADLINE, entered_rx.recv()).await.unwrap(),
            Some(())
        );
    }

    match stop {
        Stop::Forceful => handle.stop_forceful(),
        Stop::Disconnect => connection.close(0u32.into(), b"client disconnected"),
        Stop::GracefulTimeout => handle.stop_graceful(Some(Duration::from_millis(50))),
        Stop::HandlerAbort => release.notify_waiters(),
        Stop::Graceful => {
            handle.stop_graceful(None);
            assert!(
                timeout(Duration::from_millis(100), &mut server_task)
                    .await
                    .is_err(),
                "graceful shutdown must wait for accepted requests"
            );
            assert!(dropped_rx.try_recv().is_err());
            release.notify_waiters();
            for mut stream in streams.drain(..) {
                let response = timeout(DEADLINE, stream.recv_response())
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                let body = timeout(DEADLINE, async {
                    let mut body = Vec::new();
                    while let Some(mut data) = stream.recv_data().await.unwrap() {
                        body.extend_from_slice(&data.copy_to_bytes(data.remaining()));
                    }
                    body
                })
                .await
                .unwrap();
                assert_eq!(body, b"done");
            }
            // The peer has read every response and can now close its HTTP/3
            // session. Preserve the existing transport drain: finishing a local
            // request task does not mean QUIC has delivered all buffered data.
            drop(sender);
        }
    }

    for _ in 0..2 {
        assert_eq!(
            timeout(DEADLINE, dropped_rx.recv())
                .await
                .expect("connection termination must release every request"),
            Some(())
        );
    }
    if !matches!(
        stop,
        Stop::Graceful | Stop::Forceful | Stop::GracefulTimeout
    ) {
        handle.stop_forceful();
    }
    timeout(DEADLINE, server_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    connection.close(0u32.into(), b"test complete");
    driver_task.abort();
}

#[tokio::test]
async fn forceful_stop_releases_h3_requests() {
    requests_are_released(Stop::Forceful).await;
}

#[tokio::test]
async fn peer_disconnect_releases_h3_requests() {
    requests_are_released(Stop::Disconnect).await;
}

#[tokio::test]
async fn graceful_timeout_releases_h3_requests() {
    requests_are_released(Stop::GracefulTimeout).await;
}

#[tokio::test]
async fn graceful_stop_drains_h3_requests() {
    requests_are_released(Stop::Graceful).await;
}

#[tokio::test]
async fn handler_abort_releases_other_h3_requests() {
    requests_are_released(Stop::HandlerAbort).await;
}
