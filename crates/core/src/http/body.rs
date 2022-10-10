//! Http body.

use std::collections::VecDeque;
use std::error::Error as StdError;
use std::pin::Pin;
use std::task::{self, Context, Poll};

use futures_util::Stream;
use http::header::HeaderMap;
pub use hyper::body::{Body, Recv, SizeHint};

use bytes::Bytes;

/// Body for request.
#[derive(Debug)]
pub enum ReqBody {
    /// None body.
    None,
    /// Once bytes body.
    Once(Bytes),
    /// Hyper default body.
    Recv(Recv),
}

impl Default for ReqBody {
    fn default() -> Self {
        ReqBody::None
    }
}

impl Body for ReqBody {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_data(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        match &mut *self {
            ReqBody::None => Poll::Ready(None),
            ReqBody::Once(bytes) => {
                if bytes.is_empty() {
                    Poll::Ready(None)
                } else {
                    let bytes = std::mem::replace(bytes, Bytes::new());
                    Poll::Ready(Some(Ok(bytes)))
                }
            }
            ReqBody::Recv(recv) => Pin::new(recv).poll_data(cx),
        }
    }
    fn poll_trailers(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        match &mut *self {
            ReqBody::None => Poll::Ready(Ok(None)),
            ReqBody::Once(_) => Poll::Ready(Ok(None)),
            ReqBody::Recv(recv) => Pin::new(recv).poll_trailers(cx),
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            ReqBody::None => true,
            ReqBody::Once(bytes) => bytes.is_empty(),
            ReqBody::Recv(recv) => recv.is_end_stream(),
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            ReqBody::None => SizeHint::with_exact(0),
            ReqBody::Once(bytes) => SizeHint::with_exact(bytes.len() as u64),
            ReqBody::Recv(recv) => recv.size_hint(),
        }
    }
}
impl Stream for ReqBody {
    type Item = Result<Bytes, hyper::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Body::poll_data(self, cx)
    }
}

impl From<Bytes> for ReqBody {
    fn from(value: Bytes) -> ReqBody {
        ReqBody::Once(value)
    }
}
impl From<Recv> for ReqBody {
    fn from(value: Recv) -> ReqBody {
        ReqBody::Recv(value)
    }
}
impl From<String> for ReqBody {
    #[inline]
    fn from(value: String) -> ReqBody {
        ReqBody::Once(value.into())
    }
}

impl From<&'static [u8]> for ReqBody {
    fn from(value: &'static [u8]) -> ReqBody {
        ReqBody::Once(value.into())
    }
}

impl From<&'static str> for ReqBody {
    fn from(value: &'static str) -> ReqBody {
        ReqBody::Once(value.into())
    }
}

impl From<Vec<u8>> for ReqBody {
    fn from(value: Vec<u8>) -> ReqBody {
        ReqBody::Once(value.into())
    }
}

impl From<Box<[u8]>> for ReqBody {
    fn from(value: Box<[u8]>) -> ReqBody {
        ReqBody::Once(value.into())
    }
}

/// Response body type.
#[allow(clippy::type_complexity)]
#[non_exhaustive]
pub enum ResBody {
    /// None body.
    None,
    /// Once bytes body.
    Once(Bytes),
    /// Chunks body.
    Chunks(VecDeque<Bytes>),
    /// Stream body.
    Stream(Pin<Box<dyn Stream<Item = Result<Bytes, Box<dyn StdError + Send + Sync>>> + Send>>),
}
impl ResBody {
    /// Check is that body is not set.
    #[inline]
    pub fn is_none(&self) -> bool {
        matches!(*self, ResBody::None)
    }
    /// Check is that body is once.
    #[inline]
    pub fn is_once(&self) -> bool {
        matches!(*self, ResBody::Once(_))
    }
    /// Check is that body is chunks.
    #[inline]
    pub fn is_chunks(&self) -> bool {
        matches!(*self, ResBody::Chunks(_))
    }
    /// Check is that body is stream.
    #[inline]
    pub fn is_stream(&self) -> bool {
        matches!(*self, ResBody::Stream(_))
    }
    /// Get body's size.
    #[inline]
    pub fn size(&self) -> Option<u64> {
        match self {
            ResBody::None => Some(0),
            ResBody::Once(bytes) => Some(bytes.len() as u64),
            ResBody::Chunks(chunks) => Some(chunks.iter().map(|bytes| bytes.len() as u64).sum()),
            ResBody::Stream(_) => None,
        }
    }
}

impl Stream for ResBody {
    type Item = Result<Bytes, Box<dyn StdError + Send + Sync>>;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            ResBody::None => Poll::Ready(None),
            ResBody::Once(bytes) => {
                if bytes.is_empty() {
                    Poll::Ready(None)
                } else {
                    let bytes = std::mem::replace(bytes, Bytes::new());
                    Poll::Ready(Some(Ok(bytes)))
                }
            }
            ResBody::Chunks(chunks) => Poll::Ready(chunks.pop_front().map(Ok)),
            ResBody::Stream(stream) => stream.as_mut().poll_next(cx),
        }
    }
}

impl Body for ResBody {
    type Data = Bytes;
    type Error = Box<dyn StdError + Send + Sync>;

    fn poll_data(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        self.poll_next(_cx)
    }

    fn poll_trailers(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        Poll::Ready(Ok(None))
    }

    fn is_end_stream(&self) -> bool {
        match self {
            ResBody::None => true,
            ResBody::Once(bytes) => bytes.is_empty(),
            ResBody::Chunks(chunks) => chunks.is_empty(),
            ResBody::Stream(_) => false,
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            ResBody::None => SizeHint::with_exact(0),
            ResBody::Once(bytes) => SizeHint::with_exact(bytes.len() as u64),
            ResBody::Chunks(chunks) => {
                let size = chunks.iter().map(|bytes| bytes.len() as u64).sum();
                SizeHint::with_exact(size)
            }
            ResBody::Stream(_) => SizeHint::default(),
        }
    }
}
