use std::convert::Infallible;
use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};
use std::io::Error as IoError;

use crate::http::{ParseError, StatusError};
use crate::{async_trait, Depot, Request, Response, Writer};

/// BoxedError
pub type BoxedError = Box<dyn std::error::Error + Send + Sync>;

/// Errors that can happen inside salvo.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Error happened in hyper.
    Hyper(hyper::Error),
    /// Error happened when parse http.
    HttpParse(ParseError),
    /// Error from http response error status.
    HttpStatus(StatusError),
    /// Std I/O error.
    Io(IoError),
    /// SerdeJson error.
    SerdeJson(serde_json::Error),
    /// Anyhow error.
    #[cfg(feature = "anyhow")]
    #[cfg_attr(docsrs, doc(cfg(feature = "anyhow")))]
    Anyhow(anyhow::Error),
    /// Anyhow error.
    #[cfg(feature = "eyre")]
    #[cfg_attr(docsrs, doc(cfg(feature = "eyre")))]
    Eyre(eyre::Report),
    /// Custom error that does not fall under any other error kind.
    Other(BoxedError),
}

impl Error {
    /// Create a custom error.
    #[inline]
    pub fn other(error: impl Into<BoxedError>) -> Self {
        Self::Other(error.into())
    }
}
impl Display for Error {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Hyper(e) => Display::fmt(e, f),
            Self::HttpParse(e) => Display::fmt(e, f),
            Self::HttpStatus(e) => Display::fmt(e, f),
            Self::Io(e) => Display::fmt(e, f),
            Self::SerdeJson(e) => Display::fmt(e, f),
            #[cfg(feature = "anyhow")]
            Self::Anyhow(e) => Display::fmt(e, f),
            #[cfg(feature = "eyre")]
            Self::Eyre(e) => Display::fmt(e, f),
            Self::Other(e) => Display::fmt(e, f),
        }
    }
}

impl StdError for Error {}

impl From<Infallible> for Error {
    #[inline]
    fn from(infallible: Infallible) -> Error {
        match infallible {}
    }
}
impl From<hyper::Error> for Error {
    #[inline]
    fn from(e: hyper::Error) -> Error {
        Error::Hyper(e)
    }
}
impl From<ParseError> for Error {
    #[inline]
    fn from(d: ParseError) -> Error {
        Error::HttpParse(d)
    }
}
impl From<StatusError> for Error {
    #[inline]
    fn from(e: StatusError) -> Error {
        Error::HttpStatus(e)
    }
}
impl From<IoError> for Error {
    #[inline]
    fn from(e: IoError) -> Error {
        Error::Io(e)
    }
}
impl From<serde_json::Error> for Error {
    #[inline]
    fn from(e: serde_json::Error) -> Error {
        Error::SerdeJson(e)
    }
}
cfg_feature! {
    #![feature = "anyhow"]
    impl From<anyhow::Error> for Error {
        #[inline]
        fn from(e: anyhow::Error) -> Error {
            Error::Anyhow(e)
        }
    }
}
cfg_feature! {
    #![feature = "eyre"]
    impl From<eyre::Report> for Error {
        #[inline]
        fn from(e: eyre::Report) -> Error {
            Error::Eyre(e)
        }
    }
}

impl From<BoxedError> for Error {
    #[inline]
    fn from(e: BoxedError) -> Error {
        Error::Other(e)
    }
}

#[async_trait]
impl Writer for Error {
    #[inline]
    async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        let status_error = match self {
            Error::HttpStatus(e) => e,
            #[cfg(debug_assertions)]
            _ => StatusError::internal_server_error().detail(self.to_string()),
            #[cfg(not(debug_assertions))]
            _ => StatusError::internal_server_error(),
        };
        res.render(status_error);
    }
}
cfg_feature! {
    #![feature = "anyhow"]
    #[async_trait]
    impl Writer for anyhow::Error {
        #[inline]
        async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
            tracing::error!(error = ?self, "anyhow error occurred");
            #[cfg(debug_assertions)]
            res.render(StatusError::internal_server_error().detail(self.to_string()));
            #[cfg(not(debug_assertions))]
            res.render(StatusError::internal_server_error());
        }
    }
}
cfg_feature! {
    #![feature = "eyre"]
    #[async_trait]
    impl Writer for eyre::Report {
        #[inline]
        async fn write(self, _req: &mut Request, _depot: &mut Depot, res: &mut Response) {
            tracing::error!(error = ?self, "eyre error occurred");
            #[cfg(debug_assertions)]
            res.render(StatusError::internal_server_error().detail(self.to_string()));
            #[cfg(not(debug_assertions))]
            res.render(StatusError::internal_server_error());
        }
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
        let e: anyhow::Error = anyhow::anyhow!("detail message");
        e.write(&mut req, &mut depot, &mut res).await;
        assert_eq!(res.status_code, Some(StatusCode::INTERNAL_SERVER_ERROR));
    }

    #[tokio::test]
    #[cfg(feature = "eyre")]
    async fn test_eyre() {
        let mut req = Request::default();
        let mut res = Response::default();
        let mut depot = Depot::new();
        let e: eyre::Report = eyre::Report::msg("detail message");
        e.write(&mut req, &mut depot, &mut res).await;
        assert_eq!(res.status_code, Some(StatusCode::INTERNAL_SERVER_ERROR));
    }

    #[tokio::test]
    async fn test_error() {
        let mut req = Request::default();
        let mut res = Response::default();
        let mut depot = Depot::new();

        let e = Error::Other("detail message".into());
        e.write(&mut req, &mut depot, &mut res).await;
        assert_eq!(res.status_code, Some(StatusCode::INTERNAL_SERVER_ERROR));
    }
}
