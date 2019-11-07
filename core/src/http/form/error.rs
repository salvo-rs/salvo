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
pub enum Error {
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
    Httparse(httparse::Error),
    /// An I/O error.
    Io(io::Error),
    /// An error was returned from Hyper.
    Hyper(hyper::Error),
    /// An error occurred during UTF-8 processing.
    Utf8(FromUtf8Error),
    /// An error occurred during character decoding
    Decoding(Cow<'static, str>),
    General(String),

    /// Filepart is not a file
    NotAFile,
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

impl From<httparse::Error> for Error {
    fn from(err: httparse::Error) -> Error {
        Error::Httparse(err)
    }
}

impl From<hyper::Error> for Error {
    fn from(err: hyper::Error) -> Error {
        Error::Hyper(err)
    }
}

impl From<FromUtf8Error> for Error {
    fn from(err: FromUtf8Error) -> Error {
        Error::Utf8(err)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Httparse(ref e) =>
                format!("{}: {:?}", self.description(), e).fmt(f),
            Error::Io(ref e) =>
                format!("{}: {}", self.description(), e).fmt(f),
            Error::Hyper(ref e) =>
                format!("{}: {}", self.description(), e).fmt(f),
            Error::Utf8(ref e) =>
                format!("{}: {}", self.description(), e).fmt(f),
            Error::Decoding(ref e) =>
                format!("{}: {}", self.description(), e).fmt(f),
            _ => format!("{}", self.description()).fmt(f),
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&*self.description()).ok();
        if self.source().is_some() {
            write!(f, ": {:?}", self.source().unwrap()).ok(); // recurse
        }
        Ok(())
    }
}

impl StdError for Error {
    fn description(&self) -> &str{
        match *self {
            Error::NoRequestContentType => "The Hyper request did not have a Content-Type header.",
            Error::NotMultipart =>
                "The Hyper request Content-Type top-level Mime was not multipart.",
            Error::NotFormData =>
                "The Hyper request Content-Type sub-level Mime was not form-data.",
            Error::BoundaryNotSpecified =>
                "The Content-Type header failed to specify a boundary token.",
            Error::PartialHeaders => "A multipart section contained only partial headers.",
            Error::MissingDisposition =>
                "A multipart section did not have the required Content-Disposition header.",
            Error::InvalidDisposition =>
                "A multipart section did not have a valid corresponding Content-Disposition.",
            Error::NoName =>
                "A multipart section Content-Disposition header failed to specify a name.",
            Error::Eof =>
                "The request body ended prior to reaching the expected terminating boundary.",
            Error::EofInMainHeaders =>
                "The request headers ended pre-maturely.",
            Error::EofBeforeFirstBoundary =>
                "The request body ended prior to reaching the expected starting boundary.",
            Error::NoCrLfAfterBoundary =>
                "Missing CRLF after boundary.",
            Error::EofInPartHeaders =>
                "The request body ended prematurely while parsing headers of a multipart part.",
            Error::EofInFile =>
                "The request body ended prematurely while streaming a file part.",
            Error::EofInPart =>
                "The request body ended prematurely while reading a multipart part.",
            Error::Httparse(_) =>
                "A parse error occurred while parsing the headers of a multipart section.",
            Error::Io(_) => "An I/O error occurred.",
            Error::Hyper(_) => "A Hyper error occurred.",
            Error::Utf8(_) => "A UTF-8 error occurred.",
            Error::Decoding(_) => "A decoding error occurred.",
            Error::General(ref msg) => &msg,
            Error::NotAFile => "FilePart is not a file.",
        }
    }
}
