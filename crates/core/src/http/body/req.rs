use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::stream::Stream;
use hyper::body::{Body, Frame, Incoming, SizeHint};

use bytes::Bytes;

use crate::BoxedError;
use crate::fuse::{ArcFusewire, FuseEvent};

pub(crate) type BoxedBody =
    Pin<Box<dyn Body<Data = Bytes, Error = BoxedError> + Send + Sync + 'static>>;
pub(crate) type PollFrame = Poll<Option<Result<Frame<Bytes>, IoError>>>;

/// Body for HTTP request.
#[non_exhaustive]
#[derive(Default)]
pub enum ReqBody {
    /// None body.
    #[default]
    None,
    /// Once bytes body.
    Once(Bytes),
    /// Hyper default body.
    Hyper {
        /// Inner body.
        inner: Incoming,
        /// Fusewire.
        fusewire: Option<ArcFusewire>,
    },
    /// Boxed body.
    Boxed {
        /// Inner body.
        inner: BoxedBody,
        /// Fusewire.
        fusewire: Option<ArcFusewire>,
    },
}
impl ReqBody {
    #[doc(hidden)]
    pub fn set_fusewire(&mut self, value: Option<ArcFusewire>) {
        match self {
            Self::None => {}
            Self::Once(_) => {}
            Self::Hyper { fusewire, .. } => {
                *fusewire = value;
            }
            Self::Boxed { fusewire, .. } => {
                *fusewire = value;
            }
        }
    }
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
    /// Check is that body is hyper default body type.
    #[inline]
    pub fn is_hyper(&self) -> bool {
        matches!(*self, Self::Hyper { .. })
    }
    /// Check is that body is stream.
    #[inline]
    pub fn is_boxed(&self) -> bool {
        matches!(*self, Self::Boxed { .. })
    }

    /// Set body to none and returns current body.
    #[inline]
    pub fn take(&mut self) -> Self {
        std::mem::replace(self, Self::None)
    }
}

impl Body for ReqBody {
    type Data = Bytes;
    type Error = IoError;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> PollFrame {
        #[inline]
        fn through_fursewire(poll: PollFrame, fusewire: &Option<ArcFusewire>) -> PollFrame {
            match poll {
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Ready(Some(Ok(data))) => {
                    if let Some(fusewire) = fusewire {
                        fusewire.event(FuseEvent::GainFrame);
                    }
                    Poll::Ready(Some(Ok(data)))
                }
                Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
                Poll::Pending => {
                    if let Some(fusewire) = fusewire {
                        fusewire.event(FuseEvent::WaitFrame);
                    }
                    Poll::Pending
                }
            }
        }
        match &mut *self {
            Self::None => Poll::Ready(None),
            Self::Once(bytes) => {
                if bytes.is_empty() {
                    Poll::Ready(None)
                } else {
                    let bytes = std::mem::replace(bytes, Bytes::new());
                    Poll::Ready(Some(Ok(Frame::data(bytes))))
                }
            }
            Self::Hyper { inner, fusewire } => {
                let poll = Pin::new(inner)
                    .poll_frame(cx)
                    .map_err(|e| IoError::new(ErrorKind::Other, e));
                through_fursewire(poll, fusewire)
            }
            Self::Boxed { inner, fusewire } => {
                let poll = Pin::new(inner)
                    .poll_frame(cx)
                    .map_err(|e| IoError::new(ErrorKind::Other, e));
                through_fursewire(poll, fusewire)
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            Self::None => true,
            Self::Once(bytes) => bytes.is_empty(),
            Self::Hyper { inner, .. } => inner.is_end_stream(),
            Self::Boxed { inner, .. } => inner.is_end_stream(),
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            Self::None => SizeHint::with_exact(0),
            Self::Once(bytes) => SizeHint::with_exact(bytes.len() as u64),
            Self::Hyper { inner, .. } => inner.size_hint(),
            Self::Boxed { inner, .. } => inner.size_hint(),
        }
    }
}
impl Stream for ReqBody {
    type Item = IoResult<Frame<Bytes>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Body::poll_frame(self, cx) {
            Poll::Ready(Some(Ok(frame))) => Poll::Ready(Some(Ok(frame))),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(IoError::new(ErrorKind::Other, e)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl From<Bytes> for ReqBody {
    fn from(value: Bytes) -> Self {
        Self::Once(value)
    }
}
impl From<Incoming> for ReqBody {
    fn from(inner: Incoming) -> Self {
        Self::Hyper {
            inner,
            fusewire: None,
        }
    }
}
impl From<String> for ReqBody {
    #[inline]
    fn from(value: String) -> Self {
        Self::Once(value.into())
    }
}
impl TryFrom<ReqBody> for Incoming {
    type Error = crate::Error;
    fn try_from(body: ReqBody) -> Result<Self, Self::Error> {
        match body {
            ReqBody::None => Err(crate::Error::other(
                "ReqBody::None cannot convert to Incoming",
            )),
            ReqBody::Once(_) => Err(crate::Error::other(
                "ReqBody::Bytes cannot convert to Incoming",
            )),
            ReqBody::Hyper { inner, .. } => Ok(inner),
            ReqBody::Boxed { .. } => Err(crate::Error::other(
                "ReqBody::Boxed cannot convert to Incoming",
            )),
        }
    }
}

impl From<&'static [u8]> for ReqBody {
    fn from(value: &'static [u8]) -> Self {
        Self::Once(Bytes::from_static(value))
    }
}

impl From<&'static str> for ReqBody {
    fn from(value: &'static str) -> Self {
        Self::Once(Bytes::from_static(value.as_bytes()))
    }
}

impl From<Vec<u8>> for ReqBody {
    fn from(value: Vec<u8>) -> Self {
        Self::Once(value.into())
    }
}

impl<T> From<Box<T>> for ReqBody
where
    T: Into<ReqBody>,
{
    fn from(value: Box<T>) -> Self {
        (*value).into()
    }
}

cfg_feature! {
    #![feature = "quinn"]
    pub(crate) mod h3 {
        use std::boxed::Box;
        use std::pin::Pin;
        use std::task::{ready, Context, Poll};

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
                cx: &mut Context<'_>,
            ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
                let this = &mut *self;
                match ready!(this.inner.poll_recv_data(cx)) {
                    Ok(Some(buf)) => {
                        Poll::Ready(Some(Ok(Frame::data(Bytes::copy_from_slice(buf.chunk())))))
                    }
                    Ok(None) => Poll::Ready(None),
                    Err(e) => Poll::Ready(Some(Err(e.into()))),
                }
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
                ReqBody::Boxed{inner: Box::pin(value), fusewire: None}
            }
        }
    }
}

impl Debug for ReqBody {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ReqBody::None => write!(f, "ReqBody::None"),
            ReqBody::Once(value) => f.debug_tuple("ReqBody::Once").field(value).finish(),
            ReqBody::Hyper { inner, .. } => f
                .debug_struct("ReqBody::Hyper")
                .field("inner", inner)
                .finish(),
            ReqBody::Boxed { .. } => write!(f, "ReqBody::Boxed{{..}}"),
        }
    }
}
