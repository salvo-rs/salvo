#![allow(missing_docs, clippy::unwrap_used)]

use std::collections::VecDeque;
use std::convert::Infallible;
use std::io::Read;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use futures_util::Stream;
use futures_util::future::join_all;
use http_body_util::BodyExt;
use salvo_compression::{Compression, CompressionLevel};
use salvo_core::http::body::{BytesFrame, Frame, ResBody};
use salvo_core::prelude::*;
use salvo_core::test::TestClient;

#[path = "support/stream_pressure.rs"]
mod support;

#[derive(Clone, Debug, Default)]
struct Progress {
    polled: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
}
struct Source {
    frames: VecDeque<BytesFrame>,
    progress: Progress,
}
impl Stream for Source {
    type Item = Result<BytesFrame, Infallible>;
    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.progress.polled.fetch_add(1, Ordering::Relaxed);
        Poll::Ready(self.frames.pop_front().map(Ok))
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        self.progress.dropped.store(true, Ordering::Relaxed);
    }
}

#[derive(Debug)]
struct Endpoint {
    payload: Bytes,
    chunk_size: usize,
}
#[async_trait]
impl Handler for Endpoint {
    async fn handle(&self, _: &mut Request, _: &mut Depot, res: &mut Response, _: &mut FlowCtrl) {
        let progress = Progress::default();
        res.extensions.insert(progress.clone());
        let mut frames: VecDeque<_> = (0..self.payload.len())
            .step_by(self.chunk_size)
            .map(|start| {
                BytesFrame::data(
                    self.payload
                        .slice(start..(start + self.chunk_size).min(self.payload.len())),
                )
            })
            .collect();
        let mut trailers = salvo_core::http::HeaderMap::new();
        trailers.insert("x-complete", "yes".parse().unwrap());
        frames.push_back(BytesFrame(Frame::trailers(trailers)));
        res.add_header("content-type", "text/plain", true).unwrap();
        res.replace_body(ResBody::stream(Source { frames, progress }));
    }
}

fn decode(algorithm: &str, bytes: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    match algorithm {
        "gzip" => {
            flate2::read::GzDecoder::new(bytes)
                .read_to_end(&mut result)
                .unwrap();
        }
        "br" => {
            brotli::Decompressor::new(bytes, 4096)
                .read_to_end(&mut result)
                .unwrap();
        }
        "zstd" => result = zstd::stream::decode_all(bytes).unwrap(),
        _ => unreachable!(),
    }
    result
}

async fn consume(service: &Service, algorithm: &str, payload: &[u8], slow: bool) -> usize {
    let mut response = TestClient::get("http://localhost/")
        .add_header("accept-encoding", algorithm, true)
        .send(service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(response.headers()["content-encoding"], algorithm);
    let progress = response.extensions.remove::<Progress>().unwrap();
    assert_eq!(
        progress.polled.load(Ordering::Relaxed),
        0,
        "encoding must remain pull-driven"
    );
    let mut body = response.take_body();
    let mut compressed = Vec::new();
    let mut saw_trailers = false;
    while let Some(frame) = body.frame().await {
        match frame.unwrap().into_data() {
            Ok(bytes) => {
                assert!(!saw_trailers, "data followed trailers");
                compressed.extend_from_slice(&bytes);
                if slow {
                    let polled = progress.polled.load(Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    assert_eq!(
                        progress.polled.load(Ordering::Relaxed),
                        polled,
                        "input was prefetched while the consumer paused"
                    );
                }
            }
            Err(frame) => {
                assert!(!saw_trailers);
                assert_eq!(frame.into_trailers().unwrap()["x-complete"], "yes");
                saw_trailers = true;
            }
        }
    }
    assert!(saw_trailers);
    assert_eq!(decode(algorithm, &compressed), payload);
    drop(body);
    assert!(progress.dropped.load(Ordering::Relaxed));
    compressed.len()
}

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
    let payload = Bytes::from(
        (0..64 * 1024)
            .map(|i| {
                if i % 16384 < 8192 {
                    b'a' + (i % 4) as u8
                } else {
                    ((i * 17 + i / 251) % 251) as u8
                }
            })
            .collect::<Vec<_>>(),
    );
    support::header();
    for algorithm in ["gzip", "br", "zstd"] {
        for (level_name, level) in [
            ("default", CompressionLevel::Default),
            ("precise5", CompressionLevel::Precise(5)),
        ] {
            for chunk_size in [8 * 1024, 16 * 1024, 64 * 1024] {
                let compression = match algorithm {
                    "gzip" => Compression::new().disable_all().enable_gzip(level),
                    "br" => Compression::new().disable_all().enable_brotli(level),
                    "zstd" => Compression::new().disable_all().enable_zstd(level),
                    _ => unreachable!(),
                };
                let service = Service::new(Router::new().hoop(compression).get(Endpoint {
                    payload: payload.clone(),
                    chunk_size,
                }));
                let encoded_bytes = runtime.block_on(consume(&service, algorithm, &payload, false));
                for slow in [false, true] {
                    for concurrency in [1, 8] {
                        let label = format!(
                            "{algorithm}/{level_name}/{chunk_size}/{}/{concurrency}",
                            if slow { "slow" } else { "ready" }
                        );
                        support::measure(&runtime, &label, encoded_bytes, || async {
                            let sizes = join_all(
                                (0..concurrency)
                                    .map(|_| consume(&service, algorithm, &payload, slow)),
                            )
                            .await;
                            assert!(sizes.iter().all(|size| *size == encoded_bytes));
                        });
                    }
                }
                runtime.block_on(async {
                    let mut response = TestClient::get("http://localhost/")
                        .add_header("accept-encoding", algorithm, true)
                        .send(&service)
                        .await;
                    let progress = response.extensions.remove::<Progress>().unwrap();
                    let mut body = response.take_body();
                    assert!(body.frame().await.unwrap().is_ok());
                    let polled = progress.polled.load(Ordering::Relaxed);
                    drop(body);
                    tokio::task::yield_now().await;
                    assert!(progress.dropped.load(Ordering::Relaxed));
                    assert_eq!(
                        progress.polled.load(Ordering::Relaxed),
                        polled,
                        "cancelled stream read more input"
                    );
                });
            }
        }
    }
}
