//! A request that has already arrived is not dropped by a graceful stop.

#![cfg(all(feature = "server", feature = "http1", feature = "http2"))]

use std::convert::Infallible;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::Full;
use salvo_core::conn::{ConnCtrl, HttpBuilder};
use salvo_core::fuse::FuseConfig;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const DEADLINE: Duration = Duration::from_secs(5);

/// Serves one in-memory connection whose request is readable before the server looks at
/// it, while the graceful stop is already requested. Returns what the client received.
async fn serve_buffered_request() -> String {
    let (mut client, server) = tokio::io::duplex(4096);
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let stop = CancellationToken::new();
    stop.cancel();

    let service = hyper::service::service_fn(|_request| async {
        Ok::<_, Infallible>(hyper::Response::new(Full::new(Bytes::from_static(
            b"served",
        ))))
    });
    let connection = tokio::spawn(async move {
        HttpBuilder::new()
            .serve_connection(
                server,
                service,
                Some(FuseConfig::default()),
                ConnCtrl::new(),
                Some(stop),
            )
            .await
    });

    let mut received = Vec::new();
    timeout(DEADLINE, client.read_to_end(&mut received))
        .await
        .expect("the connection should be closed after the response")
        .unwrap();
    timeout(DEADLINE, connection)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    String::from_utf8(received).unwrap()
}

#[tokio::test]
async fn request_that_already_arrived_is_served_during_graceful_stop() {
    // Both the request bytes and the stop are ready at the first poll, so repeat the
    // exchange: it must never depend on which of them is looked at first.
    for attempt in 0..200 {
        let response = serve_buffered_request().await;
        assert!(
            response.starts_with("HTTP/1.1 200") && response.ends_with("served"),
            "attempt {attempt}: {response:?}"
        );
    }
}
