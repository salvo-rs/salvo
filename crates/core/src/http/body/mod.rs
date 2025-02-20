//! HTTP body.

pub use hyper::body::{Body, Frame, SizeHint};

mod req;
pub use req::ReqBody;
#[cfg(feature = "quinn")]
pub use req::h3::H3ReqBody;
mod res;
pub use hyper::body::Incoming as HyperBody;
pub use res::ResBody;
mod channel;
pub use channel::{BodyReceiver, BodySender};

use std::ops::{Deref, DerefMut};

use bytes::Bytes;

use crate::http::HeaderMap;

/// Frame with it's DATA type is [`Bytes`].
pub struct BytesFrame(pub Frame<Bytes>);
impl BytesFrame {
    /// Create a DATA frame with the provided [`Bytes`].
    pub fn data(buf: impl Into<Bytes>) -> Self {
        Self(Frame::data(buf.into()))
    }

    /// Consumes self into the buf of the DATA frame.
    ///
    /// Returns an [`Err`] containing the original [`Frame`] when frame is not a DATA frame.
    /// `Frame::is_data` can also be used to determine if the frame is a DATA frame.
    pub fn into_data(self) -> Result<Bytes, Self> {
        self.0.into_data().map_err(Self)
    }

    /// Consumes self into the buf of the trailers frame.
    ///
    /// Returns an [`Err`] containing the original [`Frame`] when frame is not a trailers frame.
    /// `Frame::is_trailers` can also be used to determine if the frame is a trailers frame.
    pub fn into_trailers(self) -> Result<HeaderMap, Self> {
        self.0.into_trailers().map_err(Self)
    }
}
impl AsRef<Frame<Bytes>> for BytesFrame {
    fn as_ref(&self) -> &Frame<Bytes> {
        &self.0
    }
}
impl Deref for BytesFrame {
    type Target = Frame<Bytes>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for BytesFrame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Bytes> for BytesFrame {
    fn from(value: Bytes) -> Self {
        Self::data(value)
    }
}
impl From<String> for BytesFrame {
    #[inline]
    fn from(value: String) -> Self {
        Self::data(value)
    }
}

impl From<&'static [u8]> for BytesFrame {
    fn from(value: &'static [u8]) -> Self {
        Self::data(value)
    }
}

impl From<&'static str> for BytesFrame {
    fn from(value: &'static str) -> Self {
        Self::data(value)
    }
}

impl From<Vec<u8>> for BytesFrame {
    fn from(value: Vec<u8>) -> Self {
        Self::data(value)
    }
}

impl<T> From<Box<T>> for BytesFrame
where
    T: Into<BytesFrame>,
{
    fn from(value: Box<T>) -> Self {
        (*value).into()
    }
}

impl From<BytesFrame> for Bytes {
    fn from(value: BytesFrame) -> Self {
        value.0.into_data().unwrap_or_default()
    }
}
