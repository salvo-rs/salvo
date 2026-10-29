//! Cancelling the serving future must release the connections it accepted.

#![cfg(all(feature = "server", feature = "http1"))]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use salvo_core::conn::Acceptor;
use salvo_core::prelude::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Notify, mpsc};
use tokio::time::timeout;

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
}

#[salvo_core::async_trait]
impl Handler for Waiting {
    async fn handle(
        &self,
        _req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        let _guard = OnDrop(self.dropped.clone());
        self.entered.send(()).unwrap();
        self.release.notified().await;
        res.render("released");
    }
}

#[tokio::test]
async fn cancelling_server_releases_an_in_flight_tcp_handler() {
    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    let address = acceptor.holdings()[0]
        .local_addr
        .clone()
        .into_std()
        .unwrap();
    let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
    let (dropped_tx, mut dropped_rx) = mpsc::unbounded_channel();
    let release = Arc::new(Notify::new());
    let server = tokio::spawn(Server::new(acceptor).try_serve(Router::new().get(Waiting {
        entered: entered_tx,
        dropped: dropped_tx,
        release: release.clone(),
    })));
    let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    timeout(DEADLINE, entered_rx.recv()).await.unwrap().unwrap();
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
    let dropped = timeout(DEADLINE, dropped_rx.recv()).await;
    // Release the original implementation's detached handler even on failure.
    release.notify_waiters();
    drop(client);
    assert_eq!(
        dropped.expect("server cancellation must release its active handlers"),
        Some(())
    );
}

struct Count(Arc<AtomicUsize>);

#[salvo_core::async_trait]
impl Handler for Count {
    async fn handle(
        &self,
        _req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        let value = self.0.fetch_add(1, Ordering::SeqCst) + 1;
        res.render(value.to_string());
    }
}

#[tokio::test]
async fn cancelling_server_closes_idle_keep_alive_connections() {
    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    let address = acceptor.holdings()[0]
        .local_addr
        .clone()
        .into_std()
        .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let server =
        tokio::spawn(Server::new(acceptor).try_serve(Router::new().get(Count(calls.clone()))));
    let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    timeout(DEADLINE, async {
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            head.push(client.read_u8().await.unwrap());
            assert!(head.len() < 16384);
        }
        assert!(head.starts_with(b"HTTP/1.1 200 "));
        assert_eq!(client.read_u8().await.unwrap(), b'1');
    })
    .await
    .unwrap();
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
    // Connection: close also makes the old detached connection finish normally,
    // so the regression never leaves a server task or socket pending.
    let mut response = Vec::new();
    if client
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .is_ok()
    {
        let _ = timeout(DEADLINE, client.read_to_end(&mut response))
            .await
            .unwrap();
    }
    assert!(
        response.is_empty(),
        "cancelled server still sent a response: {response:?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
