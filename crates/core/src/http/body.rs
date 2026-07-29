//! HTTP request and response body types.
//!
//! This module provides the body types used for HTTP requests and responses:
//!
//! # Key Types
//!
//! - [`ReqBody`]: Request body type, supports streaming and buffering
//! - [`ResBody`]: Response body type with multiple representations
//! - [`BodySender`] / [`BodyReceiver`]: Channel for streaming responses
//! - [`BytesFrame`]: A single frame of body data
//!
//! # Request Bodies
//!
//! Request bodies are read through [`ReqBody`], which supports:
//! - Streaming reads via the `Body` trait
//! - Buffered reads into `Bytes`
//! - Automatic content-type handling
//!
//! # Response Bodies
//!
//! Response bodies can be:
//! - Empty (`ResBody::None`)
//! - Single chunk (`ResBody::Once`)
//! - Multiple chunks (`ResBody::Chunks`)
//! - Streaming (`ResBody::Stream`)
//! - Error bodies (`ResBody::Error`)
//!
//! # Streaming Responses
//!
//! Use [`BodySender`] and [`BodyReceiver`] for streaming:
//!
//! ```ignore
//! use salvo_core::http::body::{BodySender, ResBody};
//!
//! let (sender, body) = BodySender::new();
//! res.body(ResBody::from(body));
//!
//! // Send data asynchronously
//! tokio::spawn(async move {
//!     sender.send_data(Bytes::from("chunk 1")).await;
//!     sender.send_data(Bytes::from("chunk 2")).await;
//! });
//! ```

pub use hyper::body::{Body, Frame, SizeHint};

mod req;
pub use req::ReqBody;
#[cfg(feature = "quinn")]
pub use req::h3::H3ReqBody;
mod res;
pub use hyper::body::Incoming as HyperBody;
pub use res::ResBody;
mod channel;
use std::ops::{Deref, DerefMut};

use bytes::Bytes;
pub use channel::{BodyReceiver, BodySender};

use crate::http::HeaderMap;

/// An HTTP body frame whose data payload type is [`Bytes`].
///
/// A frame contains either body data or trailer headers. Use [`BytesFrame::data`]
/// to create a data frame, and [`BytesFrame::into_data`] or
/// [`BytesFrame::into_trailers`] to distinguish the two frame kinds without
/// losing the original frame on a mismatch.
#[derive(Debug)]
pub struct BytesFrame(
    /// The wrapped HTTP body frame.
    pub Frame<Bytes>,
);
impl BytesFrame {
    /// Creates a data frame from a value convertible into [`Bytes`].
    pub fn data(buf: impl Into<Bytes>) -> Self {
        Self(Frame::data(buf.into()))
    }

    /// Returns the payload of a data frame.
    ///
    /// Returns `Err` containing the original frame when this is a trailers
    /// frame. [`Frame::is_data`] can be used to inspect it without consuming it.
    pub fn into_data(self) -> Result<Bytes, Self> {
        self.0.into_data().map_err(Self)
    }

    /// Returns the headers of a trailers frame.
    ///
    /// Returns `Err` containing the original frame when this is a data frame.
    /// [`Frame::is_trailers`] can be used to inspect it without consuming it.
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
    T: Into<Self>,
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
