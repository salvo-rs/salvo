//! HTTP types, request/response handling, and protocol utilities.
//!
//! This module provides the core HTTP abstractions used throughout Salvo:
//!
//! # Key Types
//!
//! | Type | Description |
//! |------|-------------|
//! | [`Request`] | Incoming HTTP request with body, headers, and metadata |
//! | [`Response`] | Outgoing HTTP response builder |
//! | [`ReqBody`] | Request body type supporting streaming |
//! | [`ResBody`] | Response body type with multiple representations |
//! | [`StatusError`] | HTTP error with status code and message |
//!
//! # Request Processing
//!
//! The [`Request`] type provides methods for:
//! - Accessing headers, method, URI, and query parameters
//! - Parsing body as JSON, form data, or multipart
//! - Extracting path parameters from the route
//! - Accessing cookies (with `cookie` feature)
//!
//! # Response Building
//!
//! The [`Response`] type supports:
//! - Setting status codes and headers
//! - Rendering various content types (JSON, HTML, text, etc.)
//! - Streaming responses
//! - Setting cookies
//!
//! # Error Handling
//!
//! Use [`StatusError`] for HTTP error responses:
//!
//! ```ignore
//! use salvo_core::http::StatusError;
//!
//! // Create common errors
//! let not_found = StatusError::not_found();
//! let bad_request = StatusError::bad_request().brief("Invalid input");
//!
//! // Custom error with details
//! let error = StatusError::internal_server_error()
//!     .brief("Database connection failed")
//!     .detail("Connection timeout after 30 seconds");
//! ```
//!
//! # Re-exports
//!
//! This module re-exports types from the `http` and `headers` crates for convenience:
//! - [`Method`], [`StatusCode`], [`HeaderMap`], [`HeaderName`], [`HeaderValue`]
//! - [`Version`], [`Mime`], and various header-related types

pub mod errors;
pub mod form;
pub mod mime;
mod range;
pub mod request;
pub mod response;
cfg_feature! {
    #![feature = "cookie"]
    pub use cookie;
}

pub use errors::{ParseError, ParseResult, StatusError, StatusResult};
pub use headers;
pub use http::method::Method;
pub use http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header, method, uri};
pub use range::HttpRange;
pub use request::Request;
pub mod body;
pub use body::{Body, ReqBody, ResBody};
pub use http::version::Version;
pub use mime::Mime;
pub use response::Response;

#[doc(hidden)]
#[must_use]
pub fn parse_accept_encoding(header: &str) -> Vec<(String, u8)> {
    let mut vec = header
        .split(',')
        .filter_map(|s| {
            let mut iter = s.trim().split(';');
            let (algo, q) = (iter.next()?, iter.next());
            let algo = algo.trim();
            let q = q
                .and_then(|q| {
                    q.trim()
                        .strip_prefix("q=")
                        .and_then(|q| q.parse::<f32>().map(|f| (f * 100.0) as u8).ok())
                })
                .unwrap_or(100u8);
            Some((algo.to_owned(), q))
        })
        .collect::<Vec<(String, u8)>>();

    vec.sort_by(|(_, a), (_, b)| match b.cmp(a) {
        std::cmp::Ordering::Equal => std::cmp::Ordering::Greater,
        other => other,
    });

    vec
}
