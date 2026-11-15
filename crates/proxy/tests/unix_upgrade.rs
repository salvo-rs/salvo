//! Real Unix socket upstreams support bidirectional upgrades and cancellation.
#![cfg(all(unix, feature = "unix-sock-client"))]
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use salvo_core::conn::{Acceptor, Listener};
use salvo_core::prelude::*;
use salvo_extra::websocket::WebSocketUpgrade;
use salvo_proxy::{Proxy, UnixSockClient};
use tokio_tungstenite::tungstenite::Message;

#[handler]
async fn websocket(req: &mut Request, res: &mut Response) -> Result<(), StatusError> {
    WebSocketUpgrade::new()
        .upgrade(req, res, |mut socket| async move {
            while let Some(Ok(message)) = socket.recv().await {
                if socket.send(message).await.is_err() {
                    break;
                }
            }
        })
        .await
}

#[tokio::test]
async fn unix_upgrade_transfers_both_ways_and_stops_with_its_front_server() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.sock.d");
    std::fs::create_dir(&path).unwrap();
    let path = path.join("server.sock");
    let upstream = Server::new(
        salvo_core::conn::UnixListener::new(path.clone())
            .bind()
            .await,
    );
    let upstream_handle = upstream.handle();
    let upstream = tokio::spawn(upstream.serve(Router::new().goal(websocket)));
    let front = TcpListener::new("127.0.0.1:0").bind().await;
    let address = front.holdings()[0].local_addr.clone().into_std().unwrap();
    let front = Server::new(front).max_connections(1);
    let handle = front.handle();
    let task = tokio::spawn(front.serve(Router::new().goal(Proxy::new(
        "http://unix/",
        UnixSockClient::with_socket_path(&path),
    ))));
    let stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let (mut socket, response) =
        tokio_tungstenite::client_async(format!("ws://{address}/"), stream)
            .await
            .unwrap();
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    socket
        .send(Message::Text("round trip".into()))
        .await
        .unwrap();
    let echoed = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(echoed.into_text().unwrap(), "round trip");
    handle.stop_forceful();
    tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
    let ended = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap();
    assert!(ended.is_none_or(|frame| frame.is_err() || frame.is_ok_and(|frame| frame.is_close())));
    upstream_handle.stop_forceful();
    tokio::time::timeout(Duration::from_secs(3), upstream)
        .await
        .unwrap()
        .unwrap();
}
