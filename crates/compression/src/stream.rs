//! Compress the body of a response.
use std::collections::VecDeque;
use std::io::{Error as IoError, Result as IoResult};
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use bytes::Bytes;
use futures_util::stream::{BoxStream, Stream};
use salvo_core::BoxedError;
use salvo_core::http::body::{Body, BytesFrame, HyperBody, ResBody};
use tokio::task::{JoinHandle, spawn_blocking};

use super::{CompressionAlgo, CompressionLevel, Encoder};

// Keep small chunks at the default/fastest level on the async task. Explicit
// compression levels can be expensive even for small inputs.
const MAX_CHUNK_SIZE_ENCODE_IN_PLACE: usize = 16 * 1024;

type Encoded = (Option<Encoder>, Bytes);

pub(super) struct EncodeStream<B> {
    encoder: Option<Encoder>,
    body: B,
    eof: bool,
    offload: bool,
    trailers: Option<BytesFrame>,
    encoding: Option<JoinHandle<IoResult<Encoded>>>,
}

impl<B> Drop for EncodeStream<B> {
    fn drop(&mut self) {
        if let Some(encoding) = &self.encoding {
            // Dropping a JoinHandle alone detaches its task. Abort queued work
            // when the response is no longer needed; a blocking task that has
            // already started must still finish on its worker thread.
            encoding.abort();
        }
    }
}

impl<B> EncodeStream<B> {
    #[allow(unused_variables)]
    pub(super) fn new(algo: CompressionAlgo, level: CompressionLevel, body: B) -> Self {
        Self {
            body,
            eof: false,
            offload: matches!(
                level,
                CompressionLevel::Minsize | CompressionLevel::Precise(_)
            ),
            trailers: None,
            encoding: None,
            encoder: Some(Encoder::new(algo, level)),
        }
    }

    fn write_chunk(&mut self, chunk: Bytes) -> IoResult<Option<Bytes>> {
        let Some(mut encoder) = self.encoder.take() else {
            return Err(IoError::other("compression encoder is unavailable"));
        };
        self.offload |= chunk.len() >= MAX_CHUNK_SIZE_ENCODE_IN_PLACE;
        if self.offload {
            self.encoding = Some(spawn_blocking(move || {
                encoder.write(&chunk)?;
                // Flushing may perform most of the compression work (especially
                // for Brotli), so keep it in the blocking task with write().
                let chunk = encoder.take()?;
                Ok((Some(encoder), chunk))
            }));
            Ok(None)
        } else {
            encoder.write(&chunk)?;
            let chunk = encoder.take()?;
            self.encoder = Some(encoder);
            Ok(Some(chunk))
        }
    }

    fn finish_encoding(&mut self) -> IoResult<Option<Bytes>> {
        let Some(encoder) = self.encoder.take() else {
            return Err(IoError::other("compression encoder is unavailable"));
        };
        if self.offload {
            self.encoding = Some(spawn_blocking(move || Ok((None, encoder.finish()?))));
            Ok(None)
        } else {
            self.eof = true;
            encoder.finish().map(Some)
        }
    }

    fn fail(&mut self, error: IoError) -> Poll<Option<IoResult<BytesFrame>>> {
        self.eof = true;
        self.encoder = None;
        self.encoding = None;
        self.trailers = None;
        Poll::Ready(Some(Err(error)))
    }
}
impl EncodeStream<BoxStream<'static, Result<Bytes, BoxedError>>> {
    #[inline]
    fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<IoResult<BytesFrame>>> {
        Stream::poll_next(Pin::new(&mut self.body), cx)
            .map_ok(BytesFrame::data)
            .map_err(IoError::other)
    }
}
impl EncodeStream<BoxStream<'static, Result<BytesFrame, BoxedError>>> {
    fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<IoResult<BytesFrame>>> {
        Stream::poll_next(Pin::new(&mut self.body), cx).map_err(IoError::other)
    }
}
impl EncodeStream<HyperBody> {
    fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<IoResult<BytesFrame>>> {
        Body::poll_frame(Pin::new(&mut self.body), cx)
            .map_ok(BytesFrame)
            .map_err(IoError::other)
    }
}
impl EncodeStream<ResBody> {
    fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<IoResult<BytesFrame>>> {
        Body::poll_frame(Pin::new(&mut self.body), cx).map_ok(BytesFrame)
    }
}
impl EncodeStream<Option<Bytes>> {
    fn poll_chunk(&mut self, _cx: &mut Context<'_>) -> Poll<Option<IoResult<BytesFrame>>> {
        Poll::Ready(self.body.take().map(|chunk| Ok(BytesFrame::data(chunk))))
    }
}
impl EncodeStream<VecDeque<Bytes>> {
    fn poll_chunk(&mut self, _cx: &mut Context<'_>) -> Poll<Option<IoResult<BytesFrame>>> {
        Poll::Ready(
            self.body
                .pop_front()
                .map(|chunk| Ok(BytesFrame::data(chunk))),
        )
    }
}

macro_rules! impl_stream {
    ($name:ty) => {
        impl Stream for EncodeStream<$name> {
            type Item = IoResult<BytesFrame>;
            fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
                let this = self.get_mut();
                loop {
                    if this.eof {
                        return Poll::Ready(this.trailers.take().map(Ok));
                    }
                    if let Some(encoding) = &mut this.encoding {
                        let result = ready!(Pin::new(encoding).poll(cx));
                        // Never poll a completed JoinHandle again, including on error.
                        this.encoding = None;
                        let result = result.map_err(|e| {
                            IoError::other(format!("blocking compression task failed: {e}"))
                        });
                        let (encoder, chunk) = match result {
                            Ok(Ok(encoded)) => encoded,
                            Ok(Err(error)) | Err(error) => return this.fail(error),
                        };
                        this.eof = encoder.is_none();
                        this.encoder = encoder;
                        if !chunk.is_empty() {
                            return Poll::Ready(Some(Ok(BytesFrame::data(chunk))));
                        }
                        continue;
                    }
                    let result = match ready!(this.poll_chunk(cx)) {
                        Some(Ok(frame)) => match frame.into_data() {
                            Ok(chunk) => this.write_chunk(chunk),
                            Err(trailers) => {
                                // Emit the final compressed bytes before forwarding trailers.
                                this.trailers = Some(trailers);
                                this.finish_encoding()
                            }
                        },
                        Some(Err(error)) => return this.fail(error),
                        None => this.finish_encoding(),
                    };
                    match result {
                        Ok(Some(chunk)) if !chunk.is_empty() => {
                            return Poll::Ready(Some(Ok(BytesFrame::data(chunk))));
                        }
                        Ok(_) => {}
                        Err(error) => return this.fail(error),
                    }
                }
            }
        }
    };
}
impl_stream!(BoxStream<'static, Result<Bytes, BoxedError>>);
impl_stream!(BoxStream<'static, Result<BytesFrame, BoxedError>>);
impl_stream!(HyperBody);
impl_stream!(ResBody);
impl_stream!(Option<Bytes>);
impl_stream!(VecDeque<Bytes>);
#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[cfg(feature = "gzip")]
    use flate2::read::GzDecoder;
    use futures_util::stream::StreamExt;

    use super::*;

    fn algorithms() -> Vec<CompressionAlgo> {
        vec![
            #[cfg(feature = "brotli")]
            CompressionAlgo::Brotli,
            #[cfg(feature = "gzip")]
            CompressionAlgo::Gzip,
            #[cfg(feature = "deflate")]
            CompressionAlgo::Deflate,
            #[cfg(feature = "zstd")]
            CompressionAlgo::Zstd,
        ]
    }

    fn decode(algo: CompressionAlgo, compressed: &[u8]) -> Vec<u8> {
        let mut decoded = Vec::new();
        match algo {
            #[cfg(feature = "brotli")]
            CompressionAlgo::Brotli => {
                brotli::Decompressor::new(compressed, 4096)
                    .read_to_end(&mut decoded)
                    .unwrap();
            }
            #[cfg(feature = "gzip")]
            CompressionAlgo::Gzip => {
                GzDecoder::new(compressed)
                    .read_to_end(&mut decoded)
                    .unwrap();
            }
            #[cfg(feature = "deflate")]
            CompressionAlgo::Deflate => {
                flate2::read::ZlibDecoder::new(compressed)
                    .read_to_end(&mut decoded)
                    .unwrap();
            }
            #[cfg(feature = "zstd")]
            CompressionAlgo::Zstd => {
                decoded = zstd::stream::decode_all(compressed).unwrap();
            }
        }
        decoded
    }

    #[tokio::test]
    async fn trailers_follow_a_complete_compressed_body() {
        use salvo_core::http::HeaderMap;
        use salvo_core::http::body::Frame;

        for algo in algorithms() {
            // Cover inline encoding, blocking encoding/finalization, and no data.
            for length in [0, 5, 32 * 1024] {
                let input = Bytes::from(vec![b'a'; length]);
                let mut trailers = HeaderMap::new();
                trailers.insert("x-checksum", "complete".parse().unwrap());
                let mut frames = Vec::new();
                if length > 0 {
                    frames.push(Ok(BytesFrame::data(input.clone())));
                }
                frames.push(Ok(BytesFrame(Frame::trailers(trailers.clone()))));
                let body: BoxStream<'static, Result<BytesFrame, BoxedError>> =
                    futures_util::stream::iter(frames).boxed();
                let mut stream = EncodeStream::new(algo, CompressionLevel::Default, body);
                let mut compressed = Vec::new();
                let mut received_trailers = None;
                while let Some(frame) = stream.next().await {
                    match frame.unwrap().into_data() {
                        Ok(chunk) => {
                            assert!(received_trailers.is_none(), "data followed trailers");
                            compressed.extend_from_slice(&chunk);
                        }
                        Err(frame) => {
                            assert!(received_trailers.is_none(), "duplicate trailers");
                            received_trailers = Some(frame.into_trailers().unwrap());
                        }
                    }
                }
                assert_eq!(decode(algo, &compressed), input);
                assert_eq!(received_trailers, Some(trailers));
                assert!(stream.next().await.is_none());
            }
        }
    }

    #[tokio::test]
    async fn blocking_jobs_include_flush_and_finalization() {
        for algo in algorithms() {
            for (level, length) in [
                (CompressionLevel::Default, 32 * 1024),
                (CompressionLevel::Minsize, 8 * 1024),
                (CompressionLevel::Precise(1), 8 * 1024),
            ] {
                let input = Bytes::from(vec![b'a'; length]);
                let mut stream = EncodeStream::new(algo, level, None::<Bytes>);
                assert!(stream.write_chunk(input.clone()).unwrap().is_none());
                let (encoder, bytes) = stream.encoding.take().unwrap().await.unwrap().unwrap();
                // The blocking job must return flushed bytes, not leave flush
                // for the async task after joining it.
                assert!(!bytes.is_empty());
                stream.encoder = encoder;
                assert!(stream.finish_encoding().unwrap().is_none());
                let (encoder, tail) = stream.encoding.take().unwrap().await.unwrap().unwrap();
                assert!(encoder.is_none());
                let mut compressed = bytes.to_vec();
                compressed.extend_from_slice(&tail);
                assert_eq!(decode(algo, &compressed), input);
            }
        }
    }

    #[tokio::test]
    async fn input_error_is_emitted_once_and_stops_polling() {
        for algo in algorithms() {
            let polls = Arc::new(AtomicUsize::new(0));
            let count = polls.clone();
            let body: BoxStream<'static, Result<BytesFrame, BoxedError>> =
                futures_util::stream::poll_fn(move |_| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Poll::Ready(Some(Err(IoError::other("source failed").into())))
                })
                .boxed();
            let mut stream = EncodeStream::new(algo, CompressionLevel::Default, body);
            assert_eq!(
                stream.next().await.unwrap().unwrap_err().to_string(),
                "source failed"
            );
            assert!(stream.next().await.is_none());
            assert_eq!(polls.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn blocking_task_error_is_emitted_once() {
        for algo in algorithms() {
            let mut stream = EncodeStream::new(algo, CompressionLevel::Default, None::<Bytes>);
            stream.encoding = Some(spawn_blocking(|| panic!("compression task failed")));
            assert!(stream.next().await.unwrap().is_err());
            assert!(stream.next().await.is_none());
        }
    }

    #[tokio::test]
    async fn compressed_stream_keeps_input_backpressure() {
        for algo in algorithms() {
            let polls = Arc::new(AtomicUsize::new(0));
            let count = polls.clone();
            let body: BoxStream<'static, Result<BytesFrame, BoxedError>> =
                futures_util::stream::poll_fn(move |_| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Poll::Ready(Some(Ok(BytesFrame::data(vec![b'a'; 32 * 1024]))))
                })
                .boxed();
            let mut stream = EncodeStream::new(algo, CompressionLevel::Default, body);
            assert!(stream.next().await.unwrap().is_ok());
            assert_eq!(polls.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn encode_in_place_threshold_covers_json_chunks() {
        const _: () = assert!(MAX_CHUNK_SIZE_ENCODE_IN_PLACE >= 8 * 1024);
    }

    #[test]
    fn dropping_stream_aborts_queued_compression_work() {
        struct Input {
            dropped: Option<tokio::sync::oneshot::Sender<()>>,
        }

        impl AsRef<[u8]> for Input {
            fn as_ref(&self) -> &[u8] {
                b"queued response body"
            }
        }

        impl Drop for Input {
            fn drop(&mut self) {
                let _ = self.dropped.take().unwrap().send(());
            }
        }

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(1)
            .build()
            .unwrap();
        runtime.block_on(async {
            for algo in algorithms() {
                let (release, blocked) = std::sync::mpsc::channel();
                let (started, wait_started) = tokio::sync::oneshot::channel();
                let blocker = spawn_blocking(move || {
                    started.send(()).unwrap();
                    // Dropping `release` on assertion failure also unblocks us.
                    let _ = blocked.recv();
                });
                wait_started.await.unwrap();

                let (dropped, wait_dropped) = tokio::sync::oneshot::channel();
                let input = Bytes::from_owner(Input {
                    dropped: Some(dropped),
                });
                let executions = Arc::new(AtomicUsize::new(0));
                let count = executions.clone();
                let mut stream = EncodeStream::new(algo, CompressionLevel::Default, None::<Bytes>);
                // Observe whether the owned task runs without doing unnecessary
                // compression. Both chunk and finalization work use this slot.
                stream.encoding = Some(spawn_blocking(move || {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok((None, input))
                }));
                drop(stream);

                release.send(()).unwrap();
                blocker.await.unwrap();
                tokio::time::timeout(std::time::Duration::from_secs(5), wait_dropped)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(executions.load(Ordering::SeqCst), 0);
            }
        });
    }

    #[cfg(feature = "gzip")]
    #[tokio::test]
    async fn test_encode_stream_once() {
        let stream = EncodeStream::new(
            CompressionAlgo::Gzip,
            CompressionLevel::Default,
            Some(Bytes::from("hello")),
        );
        let mut compressed = Vec::new();
        let mut stream = Box::pin(stream);
        while let Some(chunk) = stream.next().await {
            compressed.extend_from_slice(&chunk.unwrap().into_data().unwrap());
        }

        let mut decoder = GzDecoder::new(&compressed[..]);
        let mut decompressed = String::new();
        decoder.read_to_string(&mut decompressed).unwrap();
        assert_eq!(decompressed, "hello");
    }

    #[cfg(feature = "gzip")]
    #[tokio::test]
    async fn test_encode_stream_empty() {
        let stream = EncodeStream::new(
            CompressionAlgo::Gzip,
            CompressionLevel::Default,
            Some(Bytes::new()),
        );
        let mut compressed = Vec::new();
        let mut stream = Box::pin(stream);
        while let Some(chunk) = stream.next().await {
            compressed.extend_from_slice(&chunk.unwrap().into_data().unwrap());
        }

        let mut decoder = GzDecoder::new(&compressed[..]);
        let mut decompressed = String::new();
        decoder.read_to_string(&mut decompressed).unwrap();
        assert_eq!(decompressed, "");
    }
}
