use std::fmt::Debug;
use std::io::Error as IoError;
use std::str::Utf8Error;

use serde::de::value::Error as DeError;
use thiserror::Error;

use crate::http::{Request, Response, StatusError};
use crate::{BoxedError, Depot, Writer, async_trait};

/// Result type with `ParseError` has it's error type.
pub type ParseResult<T> = Result<T, ParseError>;

/// Errors happened when read data from http request.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ParseError {
    /// The Hyper request did not have a valid Content-Type header.
    #[error("the request did not have a valid Content-Type header")]
    InvalidContentType,

    /// The Hyper request's body is empty.
    #[error("the request's body is empty")]
    EmptyBody,

    /// The Hyper request's body is empty.
    #[error("data is not exist")]
    NotExist,

    /// Parse error when parse from str.
    #[error("Parse error when parse from str.")]
    ParseFromStr,

    /// A possible error value when converting a `StatusCode` from a `u16` or `&str`
    /// This error indicates that the supplied input was not a valid number, was less
    /// than 100, or was greater than 999.
    #[error("invalid StatusCode: {0}")]
    InvalidStatusCode(#[from] http::status::InvalidStatusCode),

    /// A possible error value when converting `Method` from bytes.
    #[error("invalid http method: {0}")]
    InvalidMethod(#[from] http::method::InvalidMethod),
    /// An error resulting from a failed attempt to construct a URI.
    #[error("invalid uri: {0}")]
    InvalidUri(#[from] http::uri::InvalidUri),
    /// An error resulting from a failed attempt to construct a URI.
    #[error("invalid uri parts: {0}")]
    InvalidUriParts(#[from] http::uri::InvalidUriParts),
    /// A possible error when converting a `HeaderName` from another type.
    #[error("invalid header name: {0}")]
    InvalidHeaderName(#[from] http::header::InvalidHeaderName),
    /// A possible error when converting a `HeaderValue` from a string or byte slice.
    #[error("invalid header value: {0}")]
    InvalidHeaderValue(#[from] http::header::InvalidHeaderValue),

    /// Deserialize error when parse from request.
    #[error("deserialize error: {0}")]
    Deserialize(#[from] DeError),

    /// DuplicateKey.
    #[error("duplicate key")]
    DuplicateKey,

    /// The Hyper request Content-Type top-level Mime was not `Multipart`.
    #[error("the Hyper request Content-Type top-level Mime was not `Multipart`.")]
    NotMultipart,

    /// The Hyper request Content-Type sub-level Mime was not `FormData`.
    #[error("the Hyper request Content-Type sub-level Mime was not `FormData`.")]
    NotFormData,

    /// InvalidRange.
    #[error("invalid range")]
    InvalidRange,

    /// An multer error.
    #[error("multer error: {0}")]
    Multer(#[from] multer::Error),

    /// An I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] IoError),

    /// An error was returned from hyper.
    #[error("hyper error: {0}")]
    Hyper(#[from] hyper::Error),

    /// An error occurred during UTF-8 processing.
    #[error("UTF-8 processing error: {0}")]
    Utf8(#[from] Utf8Error),

    /// Serde json error.
    #[error("serde json error: {0}")]
    SerdeJson(#[from] serde_json::error::Error),

    /// Custom error that does not fall under any other error kind.
    #[error("other error: {0}")]
    Other(BoxedError),
}

impl ParseError {
    /// Create a custom error.
    pub fn other(error: impl Into<BoxedError>) -> Self {
        Self::Other(error.into())
    }
}

#[async_trait]
impl Writer for ParseError {
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.render(
            StatusError::bad_request()
                .brief("parse http data failed.")
                .cause(self),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[tokio::test]
    async fn test_write_error() {
        let mut res = Response::default();
        let mut req = Request::default();
        let mut depot = Depot::new();
        let err = ParseError::EmptyBody;
        err.write(&mut req, &mut depot, &mut res).await;
    }
}
