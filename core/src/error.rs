// Copyright © 2015 by Michael Dilger (of New Zealand)
// This code is licensed under the MIT license (see LICENSE-MIT for details)

use std::borrow::Cow;
use std::fmt::{self, Display};
use std::io;
use std::string::FromUtf8Error;

use hyper;

pub type Result<T> = ::std::result::Result<T, Error>;
/// An error type for the `form_data` crate.
pub enum Error {
    /// The Hyper request did not have a Content-Type header.
    NoRequestContentType,
    /// The Hyper request did not have a Content-Type header.
    General(String),
    /// An I/O error.
    Io(io::Error),
    /// An error was returned from Hyper.
    Hyper(hyper::Error),
    /// An error occurred during UTF-8 processing.
    Utf8(FromUtf8Error),
    /// An error occurred during character decoding
    Decoding(Cow<'static, str>),
    /// A MIME multipart error
    Http(http::Error),
    SerdeJson(serde_json::error::Error),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
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

impl From<http::Error> for Error {
    fn from(err: http::Error) -> Error {
        Error::Http(err)
    }
}
impl From<serde_json::error::Error> for Error {
    fn from(err: serde_json::error::Error) -> Error {
        Error::SerdeJson(err)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::General(ref e) => e.fmt(f),
            Error::Io(ref e) => e.to_string().fmt(f),
            Error::Hyper(ref e) => e.to_string().fmt(f),
            Error::Utf8(ref e) => e.to_string().fmt(f),
            Error::Decoding(ref e) => e.to_string().fmt(f),
            Error::Http(ref e) => e.to_string().fmt(f),
            Error::SerdeJson(ref e) => e.to_string().fmt(f),
            _ => self.to_string().fmt(f),
        }
    }
}

impl std::error::Error for Error {}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&*self.to_string()).ok();
        // if self.source().is_some() {
        //     write!(f, ": {:?}", self.source().unwrap()).ok(); // recurse
        // }
        Ok(())
    }
}
