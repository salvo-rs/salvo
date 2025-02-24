use std::collections::VecDeque;
use std::fmt::{self, Debug};
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::pin::Pin;
use std::task::{self, Context, Poll, ready};

use futures_channel::{mpsc, oneshot};
use futures_util::stream::{BoxStream, FusedStream, Stream, TryStreamExt};
use hyper::body::{Body, Frame, Incoming, SizeHint};
use sync_wrapper::SyncWrapper;

use bytes::Bytes;

use crate::error::BoxedError;
use crate::http::body::{BodyReceiver, BodySender, BytesFrame};
use crate::prelude::StatusError;

/// Body for HTTP response.
#[allow(clippy::type_complexity)]
#[non_exhaustive]
#[derive(Default)]
pub enum ResBody {
    /// None body.
    #[default]
    None,
    /// Once bytes body.
    Once(Bytes),
    /// Chunks body.
    Chunks(VecDeque<Bytes>),
    /// Hyper default body.
    Hyper(Incoming),
    /// Boxed body.
    Boxed(Pin<Box<dyn Body<Data = Bytes, Error = BoxedError> + Send + Sync + 'static>>),
    /// Stream body.
    Stream(SyncWrapper<BoxStream<'static, Result<BytesFrame, BoxedError>>>),
    /// Channel body.
    Channel(BodyReceiver),
    /// Error body will be process in catcher.
    Error(StatusError),
}
impl ResBody {
    /// Check is that body is not set.
    #[inline]
    pub fn is_none(&self) -> bool {
        matches!(*self, Self::None)
    }
    /// Check is that body is once.
    #[inline]
    pub fn is_once(&self) -> bool {
        matches!(*self, Self::Once(_))
    }
    /// Check is that body is chunks.
    #[inline]
    pub fn is_chunks(&self) -> bool {
        matches!(*self, Self::Chunks(_))
    }
    /// Check is that body is hyper default body type.
    #[inline]
    pub fn is_hyper(&self) -> bool {
        matches!(*self, Self::Hyper(_))
    }
    /// Check is that body is stream.
    #[inline]
    pub fn is_boxed(&self) -> bool {
        matches!(*self, Self::Boxed(_))
    }
    /// Check is that body is stream.
    #[inline]
    pub fn is_stream(&self) -> bool {
        matches!(*self, Self::Stream(_))
    }
    /// Check is that body is stream.
    #[inline]
    pub fn is_channel(&self) -> bool {
        matches!(*self, Self::Channel { .. })
    }
    /// Check is that body is error will be process in catcher.
    pub fn is_error(&self) -> bool {
        matches!(*self, Self::Error(_))
    }

    /// Wrap a futures `Stream` in a box inside `Body`.
    pub fn stream<S, O, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<O, E>> + Send + 'static,
        O: Into<BytesFrame> + 'static,
        E: Into<BoxedError> + 'static,
    {
        let mapped = stream.map_ok(Into::into).map_err(Into::into);
        Self::Stream(SyncWrapper::new(Box::pin(mapped)))
    }

    /// Create a `Body` stream with an associated sender half.
    ///
    /// Useful when wanting to stream chunks from another thread.
    pub fn channel() -> (BodySender, Self) {
        let (data_tx, data_rx) = mpsc::channel(0);
        let (trailers_tx, trailers_rx) = oneshot::channel();

        let tx = BodySender {
            data_tx,
            trailers_tx: Some(trailers_tx),
        };
        let rx = ResBody::Channel(BodyReceiver {
            data_rx,
            trailers_rx,
        });

        (tx, rx)
    }

    /// Get body's size.
    #[inline]
    pub fn size(&self) -> Option<u64> {
        match self {
            Self::None => Some(0),
            Self::Once(bytes) => Some(bytes.len() as u64),
            Self::Chunks(chunks) => Some(chunks.iter().map(|bytes| bytes.len() as u64).sum()),
            Self::Hyper(_) => None,
            Self::Boxed(_) => None,
            Self::Stream(_) => None,
            Self::Channel { .. } => None,
            Self::Error(_) => None,
        }
    }

    /// Set body to none and returns current body.
    #[inline]
    pub fn take(&mut self) -> Self {
        std::mem::replace(self, Self::None)
    }
}

impl Body for ResBody {
    type Data = Bytes;
    type Error = IoError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, <ResBody as Body>::Error>>> {
        match self.get_mut() {
            Self::None => Poll::Ready(None),
            Self::Once(bytes) => {
                if bytes.is_empty() {
                    Poll::Ready(None)
                } else {
                    let bytes = std::mem::replace(bytes, Bytes::new());
                    Poll::Ready(Some(Ok(Frame::data(bytes))))
                }
            }
            Self::Chunks(chunks) => {
                Poll::Ready(chunks.pop_front().map(|bytes| Ok(Frame::data(bytes))))
            }
            Self::Hyper(body) => match Body::poll_frame(Pin::new(body), cx) {
                Poll::Ready(Some(Ok(frame))) => Poll::Ready(Some(Ok(frame))),
                Poll::Ready(Some(Err(e))) => {
                    Poll::Ready(Some(Err(IoError::new(ErrorKind::Other, e))))
                }
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            },
            Self::Boxed(body) => match Body::poll_frame(Pin::new(body), cx) {
                Poll::Ready(Some(Ok(frame))) => Poll::Ready(Some(Ok(frame))),
                Poll::Ready(Some(Err(e))) => {
                    Poll::Ready(Some(Err(IoError::new(ErrorKind::Other, e))))
                }
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            },
            Self::Stream(stream) => stream
                .get_mut()
                .as_mut()
                .poll_next(cx)
                .map_ok(|frame| frame.0)
                .map_err(|e| IoError::new(ErrorKind::Other, e)),
            Self::Channel(rx) => {
                if !rx.data_rx.is_terminated() {
                    if let Some(chunk) = ready!(Pin::new(&mut rx.data_rx).poll_next(cx)?) {
                        return Poll::Ready(Some(Ok(Frame::data(chunk))));
                    }
                }

                // check trailers after data is terminated
                match ready!(Pin::new(&mut rx.trailers_rx).poll(cx)) {
                    Ok(t) => Poll::Ready(Some(Ok(Frame::trailers(t)))),
                    Err(_) => Poll::Ready(None),
                }
            }
            ResBody::Error(_) => Poll::Ready(None),
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            Self::None => true,
            Self::Once(bytes) => bytes.is_empty(),
            Self::Chunks(chunks) => chunks.is_empty(),
            Self::Hyper(body) => body.is_end_stream(),
            Self::Boxed(body) => body.is_end_stream(),
            Self::Stream(_) => false,
            Self::Channel(_) => false,
            Self::Error(_) => true,
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            Self::None => SizeHint::with_exact(0),
            Self::Once(bytes) => SizeHint::with_exact(bytes.len() as u64),
            Self::Chunks(chunks) => {
                let size = chunks.iter().map(|bytes| bytes.len() as u64).sum();
                SizeHint::with_exact(size)
            }
            Self::Hyper(recv) => recv.size_hint(),
            Self::Boxed(recv) => recv.size_hint(),
            Self::Stream(_) => SizeHint::default(),
            Self::Channel { .. } => SizeHint::default(),
            Self::Error(_) => SizeHint::with_exact(0),
        }
    }
}

impl Stream for ResBody {
    type Item = IoResult<Frame<Bytes>>;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        match Body::poll_frame(self, cx) {
            Poll::Ready(Some(Ok(frame))) => Poll::Ready(Some(Ok(frame))),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(IoError::new(ErrorKind::Other, e)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl From<()> for ResBody {
    fn from(_value: ()) -> Self {
        Self::None
    }
}
impl From<Bytes> for ResBody {
    fn from(value: Bytes) -> Self {
        Self::Once(value)
    }
}
impl From<Incoming> for ResBody {
    fn from(value: Incoming) -> Self {
        Self::Hyper(value)
    }
}
impl From<String> for ResBody {
    #[inline]
    fn from(value: String) -> Self {
        Self::Once(value.into())
    }
}

impl From<&'static [u8]> for ResBody {
    fn from(value: &'static [u8]) -> Self {
        Self::Once(Bytes::from_static(value))
    }
}

impl From<&'static str> for ResBody {
    fn from(value: &'static str) -> Self {
        Self::Once(Bytes::from_static(value.as_bytes()))
    }
}

impl From<Vec<u8>> for ResBody {
    fn from(value: Vec<u8>) -> Self {
        Self::Once(value.into())
    }
}

impl<T> From<Box<T>> for ResBody
where
    T: Into<ResBody>,
{
    fn from(value: Box<T>) -> Self {
        (*value).into()
    }
}

impl Debug for ResBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "ResBody::None"),
            Self::Once(value) => f.debug_tuple("ResBody::Once").field(value).finish(),
            Self::Chunks(value) => f.debug_tuple("ResBody::Chunks").field(value).finish(),
            Self::Hyper(value) => f.debug_tuple("ResBody::Hyper").field(value).finish(),
            Self::Boxed(_) => write!(f, "ResBody::Boxed(_)"),
            Self::Stream(_) => write!(f, "ResBody::Stream(_)"),
            Self::Channel { .. } => write!(f, "ResBody::Channel{{..}}"),
            Self::Error(value) => f.debug_tuple("ResBody::Error").field(value).finish(),
        }
    }
}
