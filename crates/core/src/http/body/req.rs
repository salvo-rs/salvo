//! Http body.
use std::boxed::Box;
use std::fmt::{self, Formatter};
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::stream::Stream;
use hyper::body::{Body, Frame, Incoming, SizeHint};

use bytes::Bytes;

use crate::BoxedError;

/// Body for request.
#[derive(Default)]
pub enum ReqBody {
    /// None body.
    #[default]
    None,
    /// Once bytes body.
    Once(Bytes),
    /// Hyper default body.
    Hyper(Incoming),
    /// Inner body.
    Inner(Pin<Box<dyn Body<Data = Bytes, Error = BoxedError> + Send + Sync + Unpin + 'static>>),
}
impl fmt::Debug for ReqBody {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ReqBody::None => f.debug_tuple("ReqBody::None").finish(),
            ReqBody::Once(_) => f.debug_tuple("ReqBody::Once").finish(),
            ReqBody::Hyper(_) => f.debug_tuple("ReqBody::Hyper").finish(),
            ReqBody::Inner(_) => f.debug_tuple("ReqBody::Inner").finish(),
        }
    }
}

impl Body for ReqBody {
    type Data = Bytes;
    type Error = IoError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match &mut *self {
            ReqBody::None => Poll::Ready(None),
            ReqBody::Once(bytes) => {
                if bytes.is_empty() {
                    Poll::Ready(None)
                } else {
                    let bytes = std::mem::replace(bytes, Bytes::new());
                    Poll::Ready(Some(Ok(Frame::data(bytes))))
                }
            }
            ReqBody::Hyper(body) => Pin::new(body)
                .poll_frame(cx)
                .map_err(|e| IoError::new(ErrorKind::Other, e)),
            ReqBody::Inner(inner) => Pin::new(inner)
                .poll_frame(cx)
                .map_err(|e| IoError::new(ErrorKind::Other, e)),
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            ReqBody::None => true,
            ReqBody::Once(bytes) => bytes.is_empty(),
            ReqBody::Hyper(body) => body.is_end_stream(),
            ReqBody::Inner(inner) => inner.is_end_stream(),
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            ReqBody::None => SizeHint::with_exact(0),
            ReqBody::Once(bytes) => SizeHint::with_exact(bytes.len() as u64),
            ReqBody::Hyper(body) => body.size_hint(),
            ReqBody::Inner(inner) => inner.size_hint(),
        }
    }
}
impl Stream for ReqBody {
    type Item = IoResult<Bytes>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Body::poll_frame(self, cx) {
            Poll::Ready(Some(Ok(frame))) => Poll::Ready(frame.into_data().map(Ok).ok()),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(IoError::new(ErrorKind::Other, e)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl From<Bytes> for ReqBody {
    fn from(value: Bytes) -> ReqBody {
        ReqBody::Once(value)
    }
}
impl From<Incoming> for ReqBody {
    fn from(value: Incoming) -> ReqBody {
        ReqBody::Hyper(value)
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

cfg_feature! {
    #![feature = "quinn"]
    pub(crate) mod h3 {
        use std::boxed::Box;
        use std::pin::Pin;
        use std::task::{Context, Poll};

        use hyper::body::{Body, Frame, SizeHint};
        use salvo_http3::quic::RecvStream;

        use bytes::{Buf, Bytes};

        use crate::BoxedError;
        use crate::http::ReqBody;

        /// Http3 request body.
        pub struct H3ReqBody<S, B> {
            inner: salvo_http3::server::RequestStream<S, B>,
        }
        impl<S, B> H3ReqBody<S, B>
        where
            S: RecvStream + Send + Unpin + 'static,
            B: Buf + Send + Unpin + 'static,
        {
            /// Create new `H3ReqBody` instance.
            pub fn new(inner: salvo_http3::server::RequestStream<S, B>) -> Self {
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

            fn poll_frame(
                mut self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
                let this = &mut *self;
                let rt = tokio::runtime::Runtime::new().unwrap();
                // TODO: how to remove block?
                Poll::Ready(Some(rt.block_on(async move {
                    let buf = this.inner.recv_data().await.unwrap();
                    let buf = buf.map(|buf| Bytes::copy_from_slice(buf.chunk()));
                    Ok(Frame::data(buf.unwrap()))
                })))
            }

            fn is_end_stream(&self) -> bool {
                false
            }

            fn size_hint(&self) -> SizeHint {
                SizeHint::default()
            }
        }

        impl<S, B> From<H3ReqBody<S, B>> for ReqBody
        where
            S: RecvStream + Send + Sync +  Unpin + 'static,
            B: Buf + Send + Sync +  Unpin + 'static,
        {
            fn from(value: H3ReqBody<S, B>) -> ReqBody {
                ReqBody::Inner(Box::pin(value))
            }
        }
    }
}
