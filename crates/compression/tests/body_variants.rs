//! Compression must preserve body semantics for boxed and channel responses.

#![cfg(feature = "gzip")]

use std::io::Read;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use bytes::Bytes;
use salvo_compression::Compression;
use salvo_core::BoxedError;
use salvo_core::http::HeaderMap;
use salvo_core::http::body::{Body, Frame, ResBody};
use salvo_core::prelude::*;
use salvo_core::test::TestClient;

const LENGTH: usize = 32 * 1024;

struct Frames {
    data: Option<Bytes>,
    trailers: Option<HeaderMap>,
    fail: bool,
    dropped: Arc<AtomicUsize>,
}

impl Drop for Frames {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

impl Body for Frames {
    type Data = Bytes;
    type Error = BoxedError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        if let Some(data) = self.data.take() {
            return Poll::Ready(Some(Ok(Frame::data(data))));
        }
        if self.fail {
            self.fail = false;
            self.trailers = None;
            return Poll::Ready(Some(Err(
                std::io::Error::other("upstream body failure").into()
            )));
        }
        Poll::Ready(
            self.trailers
                .take()
                .map(|trailers| Ok(Frame::trailers(trailers))),
        )
    }
}

struct Endpoint {
    boxed: bool,
    fail: bool,
    no_transform: bool,
    dropped: Arc<AtomicUsize>,
}

#[salvo_core::async_trait]
impl Handler for Endpoint {
    async fn handle(
        &self,
        _req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        res.add_header("content-type", "text/plain", true).unwrap();
        res.add_header("content-length", LENGTH.to_string(), true)
            .unwrap();
        res.add_header("etag", "\"version\"", true).unwrap();
        res.add_header("trailer", "x-checksum", true).unwrap();
        if self.no_transform {
            res.add_header("cache-control", "no-transform", true)
                .unwrap();
        }
        let data = Bytes::from(vec![b'a'; LENGTH]);
        let mut trailers = HeaderMap::new();
        trailers.insert("x-checksum", "complete".parse().unwrap());
        if self.boxed {
            res.body(ResBody::Boxed(Box::pin(Frames {
                data: Some(data),
                trailers: Some(trailers),
                fail: self.fail,
                dropped: self.dropped.clone(),
            })));
        } else {
            let (mut sender, body) = ResBody::channel();
            let fail = self.fail;
            tokio::spawn(async move {
                if sender.send_data(data).await.is_err() {
                    return;
                }
                if fail {
                    sender.send_error(std::io::Error::other("upstream body failure"));
                } else {
                    let _ = sender.send_trailers(trailers).await;
                }
            });
            res.body(body);
        }
    }
}

async fn response(
    boxed: bool,
    fail: bool,
    no_transform: bool,
    minimum: usize,
    dropped: Arc<AtomicUsize>,
) -> Response {
    TestClient::get("http://localhost/")
        .add_header("accept-encoding", "gzip", true)
        .send(
            Router::new()
                .hoop(Compression::new().min_length(minimum))
                .get(Endpoint {
                    boxed,
                    fail,
                    no_transform,
                    dropped,
                }),
        )
        .await
}

async fn frames(mut body: ResBody) -> (Vec<u8>, HeaderMap) {
    let mut bytes = Vec::new();
    let mut trailers = None;
    while let Some(frame) =
        futures_util::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await
    {
        match frame.unwrap().into_data() {
            Ok(data) => {
                assert!(
                    trailers.is_none(),
                    "trailers must follow the final compressed bytes"
                );
                bytes.extend_from_slice(&data);
            }
            Err(frame) => {
                assert!(trailers.is_none());
                trailers = Some(frame.into_trailers().unwrap());
            }
        }
    }
    (bytes, trailers.unwrap())
}

async fn compressed(boxed: bool) {
    let mut response = response(boxed, false, false, 1, Arc::new(AtomicUsize::new(0))).await;
    assert_eq!(response.headers().get("content-encoding").unwrap(), "gzip");
    assert!(!response.headers().contains_key("content-length"));
    assert_eq!(response.headers()["etag"], "W/\"version\"");
    let (bytes, trailers) = frames(response.take_body()).await;
    assert_eq!(trailers["x-checksum"], "complete");
    let mut decoded = Vec::new();
    flate2::read::GzDecoder::new(bytes.as_slice())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, vec![b'a'; LENGTH]);
}

#[tokio::test]
async fn boxed_responses_are_compressed_with_trailers() {
    compressed(true).await;
}

#[tokio::test]
async fn channel_responses_are_compressed_with_trailers() {
    compressed(false).await;
}

#[tokio::test]
async fn variant_bypasses_preserve_original_body_and_trailers() {
    for boxed in [true, false] {
        for (no_transform, minimum) in [(true, 1), (false, LENGTH + 1)] {
            let mut response = response(
                boxed,
                false,
                no_transform,
                minimum,
                Arc::new(AtomicUsize::new(0)),
            )
            .await;
            assert!(!response.headers().contains_key("content-encoding"));
            assert_eq!(response.headers()["content-length"], LENGTH.to_string());
            assert_eq!(response.headers()["etag"], "\"version\"");
            let (bytes, trailers) = frames(response.take_body()).await;
            assert_eq!(bytes, vec![b'a'; LENGTH]);
            assert_eq!(trailers["x-checksum"], "complete");
        }
    }
}

#[tokio::test]
async fn variant_errors_remain_terminal_compression_errors() {
    for boxed in [true, false] {
        let mut response = response(boxed, true, false, 1, Arc::new(AtomicUsize::new(0))).await;
        assert_eq!(response.headers().get("content-encoding").unwrap(), "gzip");
        let mut body = response.take_body();
        loop {
            let frame = futures_util::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                .await
                .unwrap();
            if let Err(error) = frame {
                assert!(error.to_string().contains("upstream body failure"));
                break;
            }
        }
        assert!(
            futures_util::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                .await
                .is_none()
        );
    }
}

#[tokio::test]
async fn dropping_compressed_boxed_body_releases_the_source() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let response = response(true, false, false, 1, dropped.clone()).await;
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    drop(response);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}

struct ChannelEndpoint(Mutex<Option<ResBody>>);

#[salvo_core::async_trait]
impl Handler for ChannelEndpoint {
    async fn handle(
        &self,
        _req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        res.add_header("content-type", "text/plain", true).unwrap();
        res.body(self.0.lock().unwrap().take().unwrap());
    }
}

#[tokio::test]
async fn dropping_compressed_channel_body_disconnects_its_sender() {
    let (mut sender, body) = ResBody::channel();
    let mut response = TestClient::get("http://localhost/")
        .add_header("accept-encoding", "gzip", true)
        .send(
            Router::new()
                .hoop(Compression::new().min_length(1))
                .get(ChannelEndpoint(Mutex::new(Some(body)))),
        )
        .await;
    assert_eq!(response.headers()["content-encoding"], "gzip");
    let mut body = response.take_body();
    let (sent, frame) = tokio::join!(
        sender.send_data(vec![b'a'; LENGTH]),
        futures_util::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
    );
    sent.unwrap();
    assert!(frame.unwrap().unwrap().is_data());
    drop(body);
    assert!(sender.send_data("after cancellation").await.is_err());
}
