use async_trait::async_trait;
use std::convert::Infallible;
use std::error::Error as StdError;
use std::fmt;

use crate::http::StatusCode;
use crate::{Depot, Request, Response, Writer};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Errors that can happen inside salvo.
pub struct Error {
    inner: BoxError,
}

impl Error {
    pub fn new<E: Into<BoxError>>(err: E) -> Error {
        Error { inner: err.into() }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Skip showing worthless `Error { .. }` wrapper.
        fmt::Debug::fmt(&self.inner, f)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl StdError for Error {}

impl From<Infallible> for Error {
    fn from(infallible: Infallible) -> Error {
        match infallible {}
    }
}

#[async_trait]
impl Writer for Error {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
    }
}

#[test]
fn error_size_of() {
    assert_eq!(::std::mem::size_of::<Error>(), ::std::mem::size_of::<usize>() * 2);
}
