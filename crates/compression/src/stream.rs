//! Compress the body of a response.
use std::collections::VecDeque;
use std::io::{self, Error as IoError, ErrorKind, Result as IoResult};
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use bytes::Bytes;
use futures_util::stream::{BoxStream, Stream};
use tokio::task::{JoinHandle, spawn_blocking};

use salvo_core::BoxedError;
use salvo_core::http::body::{Body, BytesFrame, HyperBody};

use super::{CompressionAlgo, CompressionLevel, Encoder};

const MAX_CHUNK_SIZE_ENCODE_IN_PLACE: usize = 1024;

pub(super) struct EncodeStream<B> {
    encoder: Option<Encoder>,
    body: B,
    eof: bool,
    encoding: Option<JoinHandle<IoResult<Encoder>>>,
}

impl<B> EncodeStream<B> {
    #[allow(unused_variables)]
    pub(super) fn new(algo: CompressionAlgo, level: CompressionLevel, body: B) -> Self {
        Self {
            body,
            eof: false,
            encoding: None,
            encoder: Some(Encoder::new(algo, level)),
        }
    }
}
impl EncodeStream<BoxStream<'static, Result<Bytes, BoxedError>>> {
    #[inline]
    fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<IoResult<Bytes>>> {
        Stream::poll_next(Pin::new(&mut self.body), cx)
            .map_err(|e| IoError::new(ErrorKind::Other, e))
    }
}
impl EncodeStream<BoxStream<'static, Result<BytesFrame, BoxedError>>> {
    fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<IoResult<Bytes>>> {
        Stream::poll_next(Pin::new(&mut self.body), cx)
            .map_ok(|f| f.into_data().unwrap_or_default())
            .map_err(|e| IoError::new(ErrorKind::Other, e))
    }
}
impl EncodeStream<HyperBody> {
    fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<IoResult<Bytes>>> {
        match ready!(Body::poll_frame(Pin::new(&mut self.body), cx)) {
            Some(Ok(frame)) => Poll::Ready(frame.into_data().map(Ok).ok()),
            Some(Err(e)) => Poll::Ready(Some(Err(IoError::new(ErrorKind::Other, e)))),
            None => Poll::Ready(None),
        }
    }
}
impl EncodeStream<Option<Bytes>> {
    fn poll_chunk(&mut self, _cx: &mut Context<'_>) -> Poll<Option<IoResult<Bytes>>> {
        if let Some(body) = Pin::new(&mut self.body).take() {
            Poll::Ready(Some(Ok(body)))
        } else {
            Poll::Ready(None)
        }
    }
}
impl EncodeStream<VecDeque<Bytes>> {
    fn poll_chunk(&mut self, _cx: &mut Context<'_>) -> Poll<Option<IoResult<Bytes>>> {
        if let Some(body) = Pin::new(&mut self.body).pop_front() {
            Poll::Ready(Some(Ok(body)))
        } else {
            Poll::Ready(None)
        }
    }
}

macro_rules! impl_stream {
    ($name: ty) => {
        impl Stream for EncodeStream<$name> {
            type Item = IoResult<Bytes>;
            fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
                let this = self.get_mut();
                loop {
                    if this.eof {
                        return Poll::Ready(None);
                    }
                    if let Some(encoding) = &mut this.encoding {
                        let mut encoder = ready!(Pin::new(encoding).poll(cx)).map_err(|e| {
                            IoError::new(
                                io::ErrorKind::Other,
                                format!("blocking task was cancelled unexpectedly: {e}"),
                            )
                        })??;

                        let chunk = encoder.take()?;
                        this.encoder = Some(encoder);
                        this.encoding.take();

                        if !chunk.is_empty() {
                            return Poll::Ready(Some(Ok(chunk)));
                        }
                    }
                    match ready!(this.poll_chunk(cx)) {
                        Some(Ok(chunk)) => {
                            if let Some(mut encoder) = this.encoder.take() {
                                if chunk.len() < MAX_CHUNK_SIZE_ENCODE_IN_PLACE {
                                    encoder.write(&chunk)?;
                                    let chunk = encoder.take()?;
                                    this.encoder = Some(encoder);

                                    if !chunk.is_empty() {
                                        return Poll::Ready(Some(Ok(chunk)));
                                    }
                                } else {
                                    this.encoding = Some(spawn_blocking(move || {
                                        encoder.write(&chunk)?;
                                        Ok(encoder)
                                    }));
                                }
                            } else {
                                return Poll::Ready(Some(Ok(chunk)));
                            }
                        }
                        Some(Err(e)) => return Poll::Ready(Some(Err(e))),
                        None => {
                            if let Some(encoder) = this.encoder.take() {
                                let chunk = encoder.finish()?;
                                if chunk.is_empty() {
                                    return Poll::Ready(None);
                                } else {
                                    this.eof = true;
                                    return Poll::Ready(Some(Ok(chunk)));
                                }
                            } else {
                                return Poll::Ready(None);
                            }
                        }
                    }
                }
            }
        }
    };
}
impl_stream!(BoxStream<'static, Result<Bytes, BoxedError>>);
impl_stream!(BoxStream<'static, Result<BytesFrame, BoxedError>>);
impl_stream!(HyperBody);
impl_stream!(Option<Bytes>);
impl_stream!(VecDeque<Bytes>);
