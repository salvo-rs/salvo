//! Handshake validation must finish before publishing upgrade metadata.
#![cfg(feature = "websocket")]
#![allow(missing_docs, clippy::unwrap_used)]

use salvo_core::conn::Acceptor;
use salvo_core::prelude::*;
use salvo_extra::websocket::WebSocketUpgrade;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[handler]
async fn ws(req: &mut Request, res: &mut Response) -> Result<(), StatusError> {
    WebSocketUpgrade::new()
        .upgrade(req, res, |_| async {})
        .await
}

async fn head(extra: &str) -> String {
    head_version(extra, "HTTP/1.1").await
}

async fn head_version(extra: &str, version: &str) -> String {
    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    let addr = acceptor.holdings()[0]
        .local_addr
        .clone()
        .into_std()
        .unwrap();
    let server = tokio::spawn(Server::new(acceptor).serve(Router::new().goal(ws)));
    let mut tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
    tcp.write_all(format!("GET / {version}\r\nHost: {addr}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n{extra}\r\n").as_bytes()).await.unwrap();
    let bytes = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let mut bytes = Vec::new();
        let mut buf = [0u8; 512];
        loop {
            let n = tcp.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&buf[..n]);
            if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        bytes
    })
    .await
    .unwrap();
    server.abort();
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn failed_upgrade_has_no_success_handshake_headers() {
    let response = head_version(
        "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n",
        "HTTP/1.0",
    )
    .await;
    assert!(response.starts_with("HTTP/1.0 400"), "{response}");
    assert!(
        !response
            .to_ascii_lowercase()
            .contains("\r\nsec-websocket-accept:"),
        "{response}"
    );
    assert!(
        !response
            .to_ascii_lowercase()
            .contains("\r\nconnection: upgrade\r\n"),
        "{response}"
    );
}

#[tokio::test]
async fn websocket_rejects_invalid_and_duplicate_keys() {
    for key in ["!", "YWJj", ""] {
        let response = head(&format!(
            "Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n"
        ))
        .await;
        assert!(response.starts_with("HTTP/1.1 400"), "{key:?}: {response}");
    }
    let response = head("Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Key: YWJj\r\nSec-WebSocket-Version: 13\r\n").await;
    assert!(response.starts_with("HTTP/1.1 400"), "{response}");
}

#[tokio::test]
async fn unsupported_version_reports_the_supported_version() {
    let response =
        head("Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 12\r\n").await;
    assert!(response.starts_with("HTTP/1.1 400"), "{response}");
    assert!(
        response
            .to_ascii_lowercase()
            .contains("\r\nsec-websocket-version: 13"),
        "{response}"
    );
}
