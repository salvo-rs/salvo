use async_trait::async_trait;
use std::convert::Infallible;
use std::error::Error as StdError;
use std::fmt;

use crate::http::{Request, Response, StatusCode};
use crate::{Depot, Writer};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Errors that can happen inside salvo.
pub struct Error {
    inner: BoxError,
}

impl Error {
    #[inline]
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

#[cfg(debug_assertions)]
#[async_trait]
impl Writer for Error {
    #[inline]
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.set_http_error(crate::http::errors::InternalServerError().with_detail(&self.to_string()));
    }
}

#[cfg(not(debug_assertions))]
#[async_trait]
impl Writer for Error {
    #[inline]
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.set_status_code(StatusCode::INTERNAL_SERVER_ERROR);
    }
}

#[test]
fn error_size_of() {
    assert_eq!(::std::mem::size_of::<Error>(), ::std::mem::size_of::<usize>() * 2);
}

#[cfg(debug_assertions)]
#[cfg(feature = "anyhow")]
#[async_trait]
impl Writer for ::anyhow::Error {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.set_http_error(crate::http::errors::InternalServerError().with_detail(&self.to_string()));
    }
}

#[cfg(not(debug_assertions))]
#[cfg(feature = "anyhow")]
#[async_trait]
impl Writer for ::anyhow::Error {
    async fn write(mut self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.set_http_error(crate::http::errors::InternalServerError());
    }
}

#[cfg(test)]
mod tests {
    use crate::http::*;

    use super::*;

    #[tokio::test]
    #[cfg(feature = "anyhow")]
    async fn test_anyhow() {
        let mut req = Request::default();
        let mut res = Response::default();
        let mut depot = Depot::new();

        let err: ::anyhow::Error = Error::new("detail message").into();
        err.write(&mut req, &mut depot, &mut res).await;
        assert_eq!(res.status_code(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    }

    #[tokio::test]
    async fn test_error() {
        let mut req = Request::default();
        let mut res = Response::default();
        let mut depot = Depot::new();

        let err = Error::new("detail message");
        err.write(&mut req, &mut depot, &mut res).await;
        assert_eq!(res.status_code(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    }
}
