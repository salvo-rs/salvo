//! A graceful stop must not wait for connections that have not sent a request.

#![cfg(all(
    feature = "server",
    feature = "server-handle",
    feature = "http1",
    feature = "http2"
))]

use std::net::SocketAddr;
use std::time::Duration;

use salvo_core::conn::Acceptor;
use salvo_core::fuse::FuseConfig;
use salvo_core::prelude::*;
use salvo_core::server::ServerHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio::time::timeout;

const DEADLINE: Duration = Duration::from_secs(5);

#[handler]
async fn hello() -> &'static str {
    "hello"
}

#[handler]
async fn slow() -> &'static str {
    tokio::time::sleep(Duration::from_millis(500)).await;
    "slow"
}

async fn start(
    fuse: Option<FuseConfig>,
) -> (SocketAddr, ServerHandle, JoinHandle<std::io::Result<()>>) {
    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    let addr = acceptor.holdings()[0]
        .local_addr
        .clone()
        .into_std()
        .unwrap();
    let server = match fuse {
        Some(config) => Server::new(acceptor).fuse_config(config),
        None => Server::new(acceptor).disable_fuse(),
    };
    let handle = server.handle();
    let router = Router::new()
        .push(Router::with_path("hello").get(hello))
        .push(Router::with_path("slow").get(slow));
    (addr, handle, tokio::spawn(server.try_serve(router)))
}

/// Reads until the peer closes the connection and returns what was received.
async fn read_to_close(stream: &mut TcpStream) -> Vec<u8> {
    let mut received = Vec::new();
    timeout(DEADLINE, stream.read_to_end(&mut received))
        .await
        .expect("the server should close the connection")
        .unwrap();
    received
}

#[tokio::test]
async fn silent_connection_does_not_delay_graceful_stop() {
    // The default configuration bounds the detection read by a 30 second header
    // timeout; without the fuse it is not bounded at all.
    for fuse in [Some(FuseConfig::default()), None] {
        let (addr, handle, server) = start(fuse).await;
        let mut silent = TcpStream::connect(addr).await.unwrap();
        // Let the server accept the connection before it is asked to stop.
        tokio::time::sleep(Duration::from_millis(100)).await;

        handle.stop_graceful(None);
        timeout(DEADLINE, server)
            .await
            .expect("graceful stop should not wait for a connection without a request")
            .unwrap()
            .unwrap();
        assert!(read_to_close(&mut silent).await.is_empty());
    }
}

#[tokio::test]
async fn partial_protocol_preface_does_not_delay_graceful_stop() {
    let (addr, handle, server) = start(Some(FuseConfig::default())).await;
    let mut partial = TcpStream::connect(addr).await.unwrap();
    // A prefix of the HTTP/2 preface keeps protocol detection undecided.
    partial.write_all(b"PRI * HTTP/2").await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    handle.stop_graceful(None);
    timeout(DEADLINE, server)
        .await
        .expect("graceful stop should not wait for an undecided connection")
        .unwrap()
        .unwrap();
    assert!(read_to_close(&mut partial).await.is_empty());
}

#[tokio::test]
async fn request_in_flight_still_completes_during_graceful_stop() {
    let (addr, handle, server) = start(Some(FuseConfig::default())).await;
    let mut busy = TcpStream::connect(addr).await.unwrap();
    busy.write_all(b"GET /slow HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let _silent = TcpStream::connect(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    handle.stop_graceful(None);
    let response = String::from_utf8(read_to_close(&mut busy).await).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.ends_with("slow"), "{response}");
    timeout(DEADLINE, server).await.unwrap().unwrap().unwrap();
}
