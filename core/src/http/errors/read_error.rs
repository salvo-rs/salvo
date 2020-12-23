use std::borrow::Cow;
use std::io;
use std::str::Utf8Error;
use thiserror::Error;

use httparse;
use hyper;

#[derive(Error, Debug)]
pub enum ReadError {
    #[error("The Hyper request did not have a Content-Type header.")]
    NoRequestContentType,

    #[error("The Hyper request Content-Type top-level Mime was not `Multipart`.")]
    NotMultipart,

    #[error("The Hyper request Content-Type sub-level Mime was not `FormData`.")]
    NotFormData,

    #[error("The Content-Type header failed to specify boundary token.")]
    BoundaryNotSpecified,

    #[error("A multipart section contained only partial headers.")]
    PartialHeaders,

    #[error("A multipart section did not have the required Content-Disposition header.")]
    MissingDisposition,

    #[error("A multipart section did not have a valid corresponding Content-Disposition.")]
    InvalidDisposition,

    #[error("InvalidRange")]
    InvalidRange,

    #[error("A multipart section Content-Disposition header failed to specify a name.")]
    NoName,

    #[error("The request body ended prior to reaching the expected terminating boundary.")]
    Eof,

    #[error("EofInMainHeaders")]
    EofInMainHeaders,

    #[error("EofBeforeFirstBoundary")]
    EofBeforeFirstBoundary,

    #[error("NoCrLfAfterBoundary")]
    NoCrLfAfterBoundary,

    #[error("EofInPartHeaders")]
    EofInPartHeaders,

    #[error("EofInFile")]
    EofInFile,

    #[error("EofInPart")]
    EofInPart,

    #[error("An HTTP parsing error from a multipart section: {0}")]
    HttParse(#[from] httparse::Error),

    #[error("An I/O error: {}", _0)]
    Io(#[from] io::Error),

    #[error("An error was returned from Hyper: {0}")]
    Hyper(#[from] hyper::Error),

    #[error("An error occurred during UTF-8 processing: {0}")]
    Utf8(#[from] Utf8Error),

    #[error("An error occurred during character decoding: {0}")]
    Decoding(Cow<'static, str>),

    #[error("serde json error: {0}")]
    SerdeJson(#[from] serde_json::error::Error),

    #[error("general error: {0}")]
    General(String),

    #[error("Parse data error: {0}")]
    Parsing(String),

    #[error("Filepart is not a file")]
    NotAFile,
}
