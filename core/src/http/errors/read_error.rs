use derive_more::Display;
use std::borrow::Cow;
use std::io;
use std::str::Utf8Error;

use httparse;
use hyper;

#[derive(Debug, Display)]
pub enum ReadError {
    #[display(fmt = "The Hyper request did not have a Content-Type header")]
    NoRequestContentType,

    #[display(fmt = "The Hyper request Content-Type top-level Mime was not `Multipart`.")]
    NotMultipart,

    #[display(fmt = "The Hyper request Content-Type sub-level Mime was not `FormData`.")]
    NotFormData,

    #[display(fmt = "The Content-Type header failed to specify boundary token.")]
    BoundaryNotSpecified,

    #[display(fmt = "A multipart section contained only partial headers.")]
    PartialHeaders,

    #[display(fmt = "A multipart section did not have the required Content-Disposition header.")]
    MissingDisposition,

    #[display(fmt = "A multipart section did not have a valid corresponding Content-Disposition.")]
    InvalidDisposition,

    #[display(fmt = "InvalidRange")]
    InvalidRange,

    #[display(fmt = "A multipart section Content-Disposition header failed to specify a name.")]
    NoName,

    #[display(fmt = "The request body ended prior to reaching the expected terminating boundary.")]
    Eof,

    #[display(fmt = "EofInMainHeaders")]
    EofInMainHeaders,

    #[display(fmt = "EofBeforeFirstBoundary")]
    EofBeforeFirstBoundary,

    #[display(fmt = "NoCrLfAfterBoundary")]
    NoCrLfAfterBoundary,

    #[display(fmt = "EofInPartHeaders")]
    EofInPartHeaders,

    #[display(fmt = "EofInFile")]
    EofInFile,

    #[display(fmt = "EofInPart")]
    EofInPart,

    #[display(fmt = "An HTTP parsing error from a multipart section: {}", _0)]
    HttParse(httparse::Error),

    #[display(fmt = "An I/O error: {}", _0)]
    Io(io::Error),

    #[display(fmt = "An error was returned from Hyper: {}", _0)]
    Hyper(hyper::Error),

    #[display(fmt = "An error occurred during UTF-8 processing: {}", _0)]
    Utf8(Utf8Error),

    #[display(fmt = "An error occurred during character decoding: {}", _0)]
    Decoding(Cow<'static, str>),

    #[display(fmt = "serde json error: {}", _0)]
    SerdeJson(serde_json::error::Error),

    #[display(fmt = "general error: {}", _0)]
    General(String),

    #[display(fmt = "Parse data error: {}", _0)]
    Parsing(String),

    #[display(fmt = "Filepart is not a file")]
    NotAFile,
}

impl From<serde_json::error::Error> for ReadError {
    fn from(err: serde_json::error::Error) -> ReadError {
        ReadError::SerdeJson(err)
    }
}
impl From<io::Error> for ReadError {
    fn from(err: io::Error) -> ReadError {
        ReadError::Io(err)
    }
}

impl From<httparse::Error> for ReadError {
    fn from(err: httparse::Error) -> ReadError {
        ReadError::HttParse(err)
    }
}

impl From<hyper::Error> for ReadError {
    fn from(err: hyper::Error) -> ReadError {
        ReadError::Hyper(err)
    }
}

impl From<Utf8Error> for ReadError {
    fn from(err: Utf8Error) -> ReadError {
        ReadError::Utf8(err)
    }
}
