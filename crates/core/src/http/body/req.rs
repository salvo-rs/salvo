//! Http body.

use std::boxed::Box;
use std::collections::VecDeque;
use std::error::Error as StdError;
use std::fmt::{self, Formatter};
use std::future::Future;
use std::io::Error as IoError;
use std::pin::Pin;
use std::task::{self, Context, Poll};

use futures_util::stream::{BoxStream, Stream};
use futures_util::FutureExt;
use h3::quic::RecvStream;
use headers::Header;
use http::header::HeaderMap;
use hyper::body::{Body, Recv, SizeHint};
use pin_project::pin_project;

use bytes::{Buf, Bytes};

use crate::BoxedError;

/// Body for request.
pub enum ReqBody {
    /// None body.
    None,
    /// Once bytes body.
    Once(Bytes),
    /// Hyper default body.
    Recv(Recv),
    /// Inner body.
    Inner(Pin<Box<dyn Body<Data = Bytes, Error = BoxedError> + Send + Unpin + 'static>>),
}
impl fmt::Debug for ReqBody {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ReqBody::None => f.debug_tuple("ReqBody::None").finish(),
            ReqBody::Once(_) => f.debug_tuple("ReqBody::Once").finish(),
            ReqBody::Recv(_) => f.debug_tuple("ReqBody::Recv").finish(),
            ReqBody::Inner(_) => f.debug_tuple("ReqBody::Inner").finish(),
        }
    }
}

impl Default for ReqBody {
    fn default() -> Self {
        ReqBody::None
    }
}

impl Body for ReqBody {
    type Data = Bytes;
    type Error = BoxedError;

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
            ReqBody::Recv(recv) => Pin::new(recv).poll_data(cx).map_err(|e| e.into()),
            ReqBody::Inner(inner) => Pin::new(inner).poll_data(cx),
        }
    }
    fn poll_trailers(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        match &mut *self {
            ReqBody::None => Poll::Ready(Ok(None)),
            ReqBody::Once(_) => Poll::Ready(Ok(None)),
            ReqBody::Recv(recv) => Pin::new(recv).poll_trailers(cx).map_err(|e| e.into()),
            ReqBody::Inner(inner) => inner.as_mut().poll_trailers(cx),
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            ReqBody::None => true,
            ReqBody::Once(bytes) => bytes.is_empty(),
            ReqBody::Recv(recv) => recv.is_end_stream(),
            ReqBody::Inner(inner) => inner.is_end_stream(),
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            ReqBody::None => SizeHint::with_exact(0),
            ReqBody::Once(bytes) => SizeHint::with_exact(bytes.len() as u64),
            ReqBody::Recv(recv) => recv.size_hint(),
            ReqBody::Inner(inner) => inner.size_hint(),
        }
    }
}
impl Stream for ReqBody {
    type Item = Result<Bytes, BoxedError>;

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

impl<S, B> From<H3ReqBody<S, B>> for ReqBody
where
    S: RecvStream + Send + Unpin + 'static,
    B: Buf + Send + Unpin + 'static,
{
    fn from(value: H3ReqBody<S, B>) -> ReqBody {
        ReqBody::Inner(Box::pin(value))
    }
}

pub struct H3ReqBody<S, B> {
    inner: h3::server::RequestStream<S, B>,
}
impl<S, B> H3ReqBody<S, B>
where
    S: RecvStream + Send + Unpin + 'static,
    B: Buf + Send + Unpin + 'static,
{
    pub fn new(mut inner: h3::server::RequestStream<S, B>) -> Self {
        Self { inner }
    }
}

impl<S, B> Body for H3ReqBody<S, B>
where
    S: RecvStream + Send + Unpin,
    B: Buf + Send + Unpin,
{
    type Data = Bytes;
    type Error = BoxedError;

    fn poll_data(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        let this = &mut *self;
        let rt = tokio::runtime::Runtime::new().unwrap();
        Poll::Ready(Some(rt.block_on(async move {
            let buf = this.inner.recv_data().await.unwrap();
            let buf = buf.map(|buf| Bytes::copy_from_slice(buf.chunk()));
            Ok(buf.unwrap())
        })))
    }
    fn poll_trailers(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        Poll::Ready(Ok(None)) // TODO: how to get trailers? recv_trailers needs SendStream.
    }

    fn is_end_stream(&self) -> bool {
        false
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::default()
    }
}
