// Copyright © 2015 by Michael Dilger (of New Zealand)
// This code is licensed under the MIT license (see LICENSE-MIT for details)

use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt::{self, Display};
use std::io;
use std::string::FromUtf8Error;

use hyper;
use httparse;

/// An error type for the `form_data` crate.
pub enum ReadError {
    /// The Hyper request did not have a Content-Type header.
    NoRequestContentType,
    /// The Hyper request Content-Type top-level Mime was not `Multipart`.
    NotMultipart,
    /// The Hyper request Content-Type sub-level Mime was not `FormData`.
    NotFormData,
    /// The Content-Type header failed to specify boundary token.
    BoundaryNotSpecified,
    /// A multipart section contained only partial headers.
    PartialHeaders,
    /// A multipart section did not have the required Content-Disposition header.
    MissingDisposition,
    /// A multipart section did not have a valid corresponding Content-Disposition.
    InvalidDisposition,
    /// A multipart section Content-Disposition header failed to specify a name.
    NoName,
    /// The request body ended prior to reaching the expected terminating boundary.
    Eof,
    
    EofInMainHeaders,
    EofBeforeFirstBoundary,
    NoCrLfAfterBoundary,
    EofInPartHeaders,
    EofInFile,
    EofInPart,

    /// An HTTP parsing error from a multipart section.
    HttParse(httparse::Error),
    /// An I/O error.
    Io(io::Error),
    /// An error was returned from Hyper.
    Hyper(hyper::Error),
    /// An error occurred during UTF-8 processing.
    Utf8(FromUtf8Error),
    /// An error occurred during character decoding
    Decoding(Cow<'static, str>),
    SerdeJson(serde_json::error::Error),
    General(String),
    Parsing(String),

    /// Filepart is not a file
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

impl From<FromUtf8Error> for ReadError {
    fn from(err: FromUtf8Error) -> ReadError {
        ReadError::Utf8(err)
    }
}

impl Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ReadError::HttParse(ref e) =>
                format!("{}: {:?}", self.description(), e).fmt(f),
            ReadError::Parsing(ref e) =>
                format!("{}: {:?}", self.description(), e).fmt(f),
            ReadError::Io(ref e) =>
                format!("{}: {}", self.description(), e).fmt(f),
            ReadError::Hyper(ref e) =>
                format!("{}: {}", self.description(), e).fmt(f),
            ReadError::Utf8(ref e) =>
                format!("{}: {}", self.description(), e).fmt(f),
            ReadError::Decoding(ref e) =>
                format!("{}: {}", self.description(), e).fmt(f),
            _ => format!("{}", self.description()).fmt(f),
        }
    }
}

impl fmt::Debug for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&*self.description()).ok();
        if self.source().is_some() {
            write!(f, ": {:?}", self.source().unwrap()).ok(); // recurse
        }
        Ok(())
    }
}

impl StdError for ReadError {
    fn description(&self) -> &str{
        match *self {
            ReadError::NoRequestContentType => "The Hyper request did not have a Content-Type header.",
            ReadError::NotMultipart =>
                "The Hyper request Content-Type top-level Mime was not multipart.",
            ReadError::NotFormData =>
                "The Hyper request Content-Type sub-level Mime was not form-data.",
            ReadError::BoundaryNotSpecified =>
                "The Content-Type header failed to specify a boundary token.",
            ReadError::PartialHeaders => "A multipart section contained only partial headers.",
            ReadError::MissingDisposition =>
                "A multipart section did not have the required Content-Disposition header.",
            ReadError::InvalidDisposition =>
                "A multipart section did not have a valid corresponding Content-Disposition.",
            ReadError::NoName =>
                "A multipart section Content-Disposition header failed to specify a name.",
            ReadError::Eof =>
                "The request body ended prior to reaching the expected terminating boundary.",
            ReadError::EofInMainHeaders =>
                "The request headers ended pre-maturely.",
            ReadError::EofBeforeFirstBoundary =>
                "The request body ended prior to reaching the expected starting boundary.",
            ReadError::NoCrLfAfterBoundary =>
                "Missing CRLF after boundary.",
            ReadError::EofInPartHeaders =>
                "The request body ended prematurely while parsing headers of a multipart part.",
            ReadError::EofInFile =>
                "The request body ended prematurely while streaming a file part.",
            ReadError::EofInPart =>
                "The request body ended prematurely while reading a multipart part.",
            ReadError::HttParse(_) =>
                "A parse error occurred while parsing the headers of a multipart section.",
            ReadError::Io(_) => "An I/O error occurred.",
            ReadError::Hyper(_) => "A Hyper error occurred.",
            ReadError::Utf8(_) => "A UTF-8 error occurred.",
            ReadError::Decoding(_) => "A decoding error occurred.",
            ReadError::General(ref msg) => &msg,
            ReadError::Parsing(ref msg) => &msg,
            ReadError::NotAFile => "FilePart is not a file.",
            ReadError::SerdeJson(_) => "A serde json error occurred.",
        }
    }
}
