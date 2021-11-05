use std::io;
use std::str::Utf8Error;

use async_trait::async_trait;
use thiserror::Error;

use crate::http::errors::*;
use crate::{Depot, Request, Response, Writer};

/// ReadError, errors happened when read data from http request.
#[derive(Error, Debug)]
pub enum ReadError {
    /// The Hyper request did not have a valid Content-Type header.
    #[error("The Hyper request did not have a valid Content-Type header.")]
    InvalidContentType,
    
    /// The Hyper request's body is empty.
    #[error("The Hyper request's body is empty.")]
    EmptyBody,
    
    /// Parse error when pase from str.
    #[error("Parse error when pase from str.")]
    ParseFromStr,

    /// The Hyper request Content-Type top-level Mime was not `Multipart`.
    #[error("The Hyper request Content-Type top-level Mime was not `Multipart`.")]
    NotMultipart,

    /// The Hyper request Content-Type sub-level Mime was not `FormData`.
    #[error("The Hyper request Content-Type sub-level Mime was not `FormData`.")]
    NotFormData,

    /// InvalidRange.
    #[error("InvalidRange")]
    InvalidRange,

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

    /// Serde json error.
    #[error("Serde json error: {0}")]
    SerdeJson(#[from] serde_json::error::Error),
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
