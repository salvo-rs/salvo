//! Error types and handling for Salvo.
//!
//! This module provides the core error type [`Error`] used throughout Salvo,
//! along with conversions from common error types and rendering support.
//!
//! # Error Type
//!
//! The [`Error`] enum wraps various error types that can occur during request
//! handling:
//!
//! - HTTP parsing errors
//! - I/O errors
//! - JSON serialization errors
//! - HTTP status errors
//! - Custom errors via [`BoxedError`]
//!
//! # Error Handling in Handlers
//!
//! Handlers can return `Result<T, Error>` or `Result<T, impl Writer>`:
//!
//! ```ignore
//! use salvo_core::prelude::*;
//!
//! #[handler]
//! async fn example() -> Result<&'static str, salvo_core::Error> {
//!     // Errors are automatically converted and rendered
//!     Ok("Success")
//! }
//! ```
//!
//! # Custom Error Types
//!
//! Custom error types can be converted to [`Error`] using the [`other`](Error::other)
//! method or by implementing `From`:
//!
//! ```ignore
//! use salvo_core::Error;
//!
//! let custom_error = Error::other(MyCustomError::new("something failed"));
//! ```
//!
//! # Integration with anyhow and eyre
//!
//! With the `anyhow` or `eyre` features enabled, errors from those crates
//! are automatically convertible to [`Error`] and will render as 500 Internal
//! Server Error responses.

use std::convert::Infallible;
use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};
use std::io::Error as IoError;

use crate::http::{ParseError, StatusError};
use crate::{Response, Scribe};

/// A boxed error type for dynamic error handling.
///
/// This type alias provides a convenient way to work with any error type
/// that implements `std::error::Error + Send + Sync`.
pub type BoxedError = Box<dyn StdError + Send + Sync>;

/// The main error type used throughout Salvo.
///
/// This enum encompasses all error types that can occur during request processing,
/// from low-level I/O errors to HTTP-specific errors.
///
/// # Rendering
///
/// `Error` implements [`Scribe`], which means it can be rendered directly to a
/// response. Most error variants render as a 500 Internal Server Error, except
/// for `HttpStatus` which uses the status code from the contained [`StatusError`].
///
/// # Error Conversion
///
/// Common error types automatically convert to `Error`:
///
/// ```ignore
/// use salvo_core::Error;
///
/// // I/O errors
/// let io_err: Error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found").into();
///
/// // JSON errors
/// let json_err: Error = serde_json::from_str::<()>("invalid").unwrap_err().into();
///
/// // Custom errors
/// let custom: Error = Error::other(MyError);
/// ```
///
/// # Feature-Gated Variants
///
/// Some variants are only available with specific features:
/// - `quinn`: HTTP/3 error variants
/// - `anyhow`: `anyhow::Error` support
/// - `eyre`: `eyre::Report` support
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Error from the hyper HTTP library.
    ///
    /// These errors typically occur during low-level HTTP processing.
    Hyper(hyper::Error),
    /// HTTP parsing error.
    ///
    /// Occurs when request data cannot be parsed correctly.
    HttpParse(ParseError),
    /// HTTP status error with an associated status code.
    ///
    /// Unlike other variants, this error uses its contained status code
    /// when rendering to a response.
    HttpStatus(StatusError),
    /// Standard I/O error.
    ///
    /// Common for file operations and network issues.
    Io(IoError),
    /// JSON serialization/deserialization error.
    SerdeJson(serde_json::Error),
    /// Invalid URI error.
    ///
    /// Occurs when a URI cannot be parsed.
    InvalidUri(http::uri::InvalidUri),
    /// HTTP/3 connection error (requires `quinn` feature).
    #[cfg(feature = "quinn")]
    #[cfg_attr(docsrs, doc(cfg(feature = "quinn")))]
    H3Connection(salvo_http3::error::ConnectionError),
    /// HTTP/3 stream error (requires `quinn` feature).
    #[cfg(feature = "quinn")]
    #[cfg_attr(docsrs, doc(cfg(feature = "quinn")))]
    H3Stream(salvo_http3::error::StreamError),
    /// HTTP/3 datagram send error (requires `quinn` feature).
    #[cfg(feature = "quinn")]
    #[cfg_attr(docsrs, doc(cfg(feature = "quinn")))]
    H3SendDatagram(h3_datagram::datagram_handler::SendDatagramError),
    /// Error from the anyhow crate (requires `anyhow` feature).
    #[cfg(feature = "anyhow")]
    #[cfg_attr(docsrs, doc(cfg(feature = "anyhow")))]
    Anyhow(anyhow::Error),
    /// Error from the eyre crate (requires `eyre` feature).
    #[cfg(feature = "eyre")]
    #[cfg_attr(docsrs, doc(cfg(feature = "eyre")))]
    Eyre(eyre::Report),
    /// Any other error type wrapped as a boxed trait object.
    ///
    /// Use [`Error::other`] to create this variant from any error type.
    Other(BoxedError),
}

impl Error {
    /// Creates an `Error` from any error type.
    ///
    /// This is useful for wrapping custom error types that don't have
    /// a dedicated variant in the `Error` enum.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use salvo_core::Error;
    ///
    /// #[derive(Debug)]
    /// struct MyError(String);
    ///
    /// impl std::fmt::Display for MyError {
    ///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    ///         write!(f, "{}", self.0)
    ///     }
    /// }
    ///
    /// impl std::error::Error for MyError {}
    ///
    /// let error = Error::other(MyError("something went wrong".into()));
    /// ```
    #[inline]
    pub fn other(error: impl Into<BoxedError>) -> Self {
        Self::Other(error.into())
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Hyper(e) => Display::fmt(e, f),
            Self::HttpParse(e) => Display::fmt(e, f),
            Self::HttpStatus(e) => Display::fmt(e, f),
            Self::Io(e) => Display::fmt(e, f),
            Self::SerdeJson(e) => Display::fmt(e, f),
            Self::InvalidUri(e) => Display::fmt(e, f),
            #[cfg(feature = "quinn")]
            Self::H3Connection(e) => Display::fmt(e, f),
            #[cfg(feature = "quinn")]
            Self::H3Stream(e) => Display::fmt(e, f),
            #[cfg(feature = "quinn")]
            Self::H3SendDatagram(e) => Display::fmt(e, f),
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
    fn from(infallible: Infallible) -> Self {
        match infallible {}
    }
}
impl From<hyper::Error> for Error {
    #[inline]
    fn from(e: hyper::Error) -> Self {
        Self::Hyper(e)
    }
}
impl From<ParseError> for Error {
    #[inline]
    fn from(d: ParseError) -> Self {
        Self::HttpParse(d)
    }
}
impl From<StatusError> for Error {
    #[inline]
    fn from(e: StatusError) -> Self {
        Self::HttpStatus(e)
    }
}
impl From<IoError> for Error {
    #[inline]
    fn from(e: IoError) -> Self {
        Self::Io(e)
    }
}
impl From<http::uri::InvalidUri> for Error {
    #[inline]
    fn from(e: http::uri::InvalidUri) -> Self {
        Self::InvalidUri(e)
    }
}
impl From<serde_json::Error> for Error {
    #[inline]
    fn from(e: serde_json::Error) -> Self {
        Self::SerdeJson(e)
    }
}
cfg_feature! {
    #![feature = "quinn"]
    impl From<salvo_http3::error::ConnectionError> for Error {
        #[inline]
        fn from(e: salvo_http3::error::ConnectionError) -> Self {
            Self::H3Connection(e)
        }
    }
    impl From<salvo_http3::error::StreamError> for Error {
        #[inline]
        fn from(e: salvo_http3::error::StreamError) -> Self {
            Self::H3Stream(e)
        }
    }
    impl From<h3_datagram::datagram_handler::SendDatagramError> for Error {
        #[inline]
        fn from(e: h3_datagram::datagram_handler::SendDatagramError) -> Self {
            Self::H3SendDatagram(e)
        }
    }
}
cfg_feature! {
    #![feature = "anyhow"]
    impl From<anyhow::Error> for Error {
        #[inline]
        fn from(e: anyhow::Error) -> Self {
            Self::Anyhow(e)
        }
    }
}
cfg_feature! {
    #![feature = "eyre"]
    impl From<eyre::Report> for Error {
        #[inline]
        fn from(e: eyre::Report) -> Self {
            Self::Eyre(e)
        }
    }
}

impl From<BoxedError> for Error {
    #[inline]
    fn from(e: BoxedError) -> Self {
        Self::Other(e)
    }
}

impl Scribe for Error {
    fn render(self, res: &mut Response) {
        let status_error = match self {
            Self::HttpStatus(e) => e,
            _ => StatusError::internal_server_error().cause(self),
        };
        res.render(status_error);
    }
}
cfg_feature! {
    #![feature = "anyhow"]
    impl Scribe for anyhow::Error {
        #[inline]
        fn render(self, res: &mut Response) {
            tracing::error!(error = ?self, "anyhow error occurred");
            res.render(StatusError::internal_server_error().origin(self));
        }
    }
}
cfg_feature! {
    #![feature = "eyre"]
    impl Scribe for eyre::Report {
        #[inline]
        fn render(self, res: &mut Response) {
            tracing::error!(error = ?self, "eyre error occurred");
            res.render(StatusError::internal_server_error().cause(self));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::http::*;
    use crate::{Depot, Writer};

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

        let e = Error::Other(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "detail message",
        )));
        e.write(&mut req, &mut depot, &mut res).await;
        assert_eq!(res.status_code, Some(StatusCode::INTERNAL_SERVER_ERROR));
    }

    #[test]
    fn test_error_from() {
        use std::io;

        let err: Error = io::Error::new(io::ErrorKind::Other, "oh no!").into();
        assert!(matches!(err, Error::Io(_)));

        let err: Error = ParseError::ParseFromStr.into();
        assert!(matches!(err, Error::HttpParse(_)));

        let err: Error = StatusError::bad_request().into();
        assert!(matches!(err, Error::HttpStatus(_)));

        let err: Error = serde_json::from_str::<serde_json::Value>("{")
            .unwrap_err()
            .into();
        assert!(matches!(err, Error::SerdeJson(_)));

        let err: Error = http::Uri::from_str("ht tp://host.com").unwrap_err().into();
        assert!(matches!(err, Error::InvalidUri(_)));

        let err: Error = Error::other(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "custom error",
        )));
        assert!(matches!(err, Error::Other(_)));
    }

    #[test]
    fn test_error_display() {
        use std::io;

        let err: Error = io::Error::new(io::ErrorKind::Other, "io error").into();
        assert_eq!(format!("{}", err), "io error");

        let err: Error = ParseError::ParseFromStr.into();
        assert_eq!(format!("{}", err), "Parse error when parse from str.");

        let err: Error = StatusError::bad_request().brief("status error").into();
        assert!(format!("{}", err).contains("status error"));
    }

    #[tokio::test]
    async fn test_error_scribe() {
        let mut req = Request::default();
        let mut res = Response::default();
        let mut depot = Depot::new();

        let e = Error::from(StatusError::bad_request());
        e.write(&mut req, &mut depot, &mut res).await;
        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));

        let mut res = Response::default();
        let e = std::io::Error::new(std::io::ErrorKind::Other, "io error");
        Error::from(e).write(&mut req, &mut depot, &mut res).await;
        assert_eq!(res.status_code, Some(StatusCode::INTERNAL_SERVER_ERROR));
    }
}
