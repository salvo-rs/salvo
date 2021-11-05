use std::borrow::Cow;
use std::io;
use std::str::Utf8Error;

use async_trait::async_trait;
use thiserror::Error;

use crate::http::errors::*;
use crate::{Depot, Request, Response, Writer};

/// ReadError, errors happened when read data from http request.
#[derive(Error, Debug)]
pub enum ReadError {
    /// The Hyper request did not have a Content-Type header.
    #[error("The Hyper request did not have a Content-Type header.")]
    NoRequestContentType,

    /// The Hyper request Content-Type top-level Mime was not `Multipart`.
    #[error("The Hyper request Content-Type top-level Mime was not `Multipart`.")]
    NotMultipart,

    /// The Hyper request Content-Type sub-level Mime was not `FormData`.
    #[error("The Hyper request Content-Type sub-level Mime was not `FormData`.")]
    NotFormData,

    /// The Content-Type header failed to specify boundary token.
    #[error("The Content-Type header failed to specify boundary token.")]
    BoundaryNotSpecified,

    /// A multipart section contained only partial headers.
    #[error("A multipart section contained only partial headers.")]
    PartialHeaders,

    /// A multipart section did not have the required Content-Disposition header.
    #[error("A multipart section did not have the required Content-Disposition header.")]
    MissingDisposition,

    /// A multipart section did not have a valid corresponding Content-Disposition.
    #[error("A multipart section did not have a valid corresponding Content-Disposition.")]
    InvalidDisposition,

    /// InvalidRange.
    #[error("InvalidRange")]
    InvalidRange,

    /// A multipart section Content-Disposition header failed to specify a name.
    #[error("A multipart section Content-Disposition header failed to specify a name.")]
    NoName,

    /// The request body ended prior to reaching the expected terminating boundary.
    #[error("The request body ended prior to reaching the expected terminating boundary.")]
    Eof,

    /// EofInMainHeaders.
    #[error("EofInMainHeaders")]
    EofInMainHeaders,

    /// EofBeforeFirstBoundary.
    #[error("EofBeforeFirstBoundary")]
    EofBeforeFirstBoundary,

    /// NoCrLfAfterBoundary.
    #[error("NoCrLfAfterBoundary")]
    NoCrLfAfterBoundary,

    /// EofInPartHeaders.
    #[error("EofInPartHeaders")]
    EofInPartHeaders,

    /// EofInFile.
    #[error("EofInFile")]
    EofInFile,

    /// EofInPart.
    #[error("EofInPart")]
    EofInPart,

    /// An HTTP parsing error from a multipart section.
    #[error("An HTTP parsing error from a multipart section: {0}")]
    HttParse(#[from] httparse::Error),

    /// An multer error.
    #[error("An multer error from: {0}")]
    Multer(#[from] multer::Error),

    /// An I/O error.
    #[error("An I/O error: {}", _0)]
    Io(#[from] io::Error),

    /// An error was returned from hyper.
    #[error("An error was returned from hyper: {0}")]
    Hyper(#[from] hyper::Error),

    /// An error occurred during UTF-8 processing.
    #[error("An error occurred during UTF-8 processing: {0}")]
    Utf8(#[from] Utf8Error),

    /// An error occurred during character decoding.
    #[error("An error occurred during character decoding: {0}")]
    Decoding(Cow<'static, str>),

    /// Serde json error.
    #[error("Serde json error: {0}")]
    SerdeJson(#[from] serde_json::error::Error),

    /// General error.
    #[error("General error: {0}")]
    General(String),

    /// Parse data error.
    #[error("Parse data error: {0}")]
    Parsing(String),

    /// Filepart is not a file.
    #[error("Filepart is not a file")]
    NotAFile,
}

#[async_trait]
impl Writer for ReadError {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.set_http_error(
            InternalServerError()
                .with_summary("http read error happened")
                .with_detail("there is no more detailed explanation."),
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
        let err = ReadError::NoName;
        err.write(&mut req, &mut depot, &mut res).await;
    }
}
