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
/// Cookie security policy helpers.
pub mod secure_cookie;
cfg_feature! {
    #![feature = "cookie"]
    pub use cookie;
}

pub use errors::{ParseError, ParseResult, StatusError, StatusResult};
pub use form::FormLimits;
cfg_feature! {
    #![feature = "rfc9457"]
    pub use errors::{NoExtensions, PROBLEM_JSON, PlainProblem, Problem};
}
pub use headers;
pub use http::method::Method;
pub use http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header, method, uri};
pub use range::HttpRange;
pub use request::Request;
pub use secure_cookie::SecureCookiePolicy;
pub mod body;
pub use body::{Body, ReqBody, ResBody};
pub use http::version::Version;
pub use mime::Mime;
pub use response::Response;

#[doc(hidden)]
pub fn append_vary_header(headers: &mut HeaderMap, vary_by: &'static str) {
    if headers.contains_key(header::VARY) {
        let already_varies = headers
            .get_all(header::VARY)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
            .any(|value| {
                let value = value.trim();
                value == "*" || value.eq_ignore_ascii_case(vary_by)
            });
        if already_varies {
            return;
        }
    }
    headers.append(header::VARY, HeaderValue::from_static(vary_by));
}

#[doc(hidden)]
#[must_use]
pub fn parse_accept_encoding(header: &str) -> Vec<(String, u8)> {
    parse_accept_encoding_impl(header, 100, |value| {
        value.parse::<f32>().map(|f| (f * 100.0) as u8).ok()
    })
}

/// Parse encoding preferences using the full three-decimal precision of RFC 9110.
///
/// Valid quality values are represented as integers from 0 through 1000.
/// [`parse_accept_encoding`] retains its original percentage-based return values.
#[doc(hidden)]
#[must_use]
pub fn parse_accept_encoding_with_quality(header: &str) -> Vec<(String, u16)> {
    parse_accept_encoding_impl(header, 1000, parse_quality_thousandths)
}

fn parse_quality_thousandths(value: &str) -> Option<u16> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if fraction.len() <= 3
        && fraction.bytes().all(|byte| byte.is_ascii_digit())
        && (whole == "0" || whole == "1" && fraction.bytes().all(|byte| byte == b'0'))
    {
        let mut quality = if whole == "1" { 1000 } else { 0 };
        let mut scale = 100;
        for digit in fraction.bytes() {
            quality += u16::from(digit - b'0') * scale;
            scale /= 10;
        }
        return Some(quality);
    }

    // Keep the previous recovery policy for non-conforming weights, including
    // float syntax, out-of-range values, and failed parses. Only valid RFC
    // qvalues gain precision; malformed input must not acquire new meaning.
    value
        .parse::<f32>()
        .map(|f| u16::from((f * 100.0) as u8) * 10)
        .ok()
}

fn parse_accept_encoding_impl<Q: Copy + Ord>(
    header: &str,
    default_quality: Q,
    parse_quality: impl Fn(&str) -> Option<Q>,
) -> Vec<(String, Q)> {
    let mut vec = header
        .split(',')
        .filter_map(|s| {
            let mut iter = s.trim().split(';');
            let (algo, q) = (iter.next()?, iter.next());
            // Content codings and the `q` parameter name are case-insensitive.
            let algo = algo.trim().to_ascii_lowercase();
            let q = q
                .and_then(|q| {
                    let (name, value) = q.trim().split_once('=')?;
                    if !name.eq_ignore_ascii_case("q") {
                        return None;
                    }
                    parse_quality(value)
                })
                .unwrap_or(default_quality);
            Some((algo, q))
        })
        .collect::<Vec<(String, Q)>>();

    vec.sort_by(|(_, a), (_, b)| b.cmp(a));

    vec
}

#[cfg(test)]
mod tests {
    use super::{parse_accept_encoding, parse_accept_encoding_with_quality};

    #[test]
    fn malformed_encoding_weights_keep_the_previous_recovery_policy() {
        for value in [
            "invalid", "NaN", "-1", "2", "1e-3", "0.0001", "0.1234", "1.001", "1e3",
        ] {
            let header = format!("gzip;q={value}");
            let legacy = parse_accept_encoding(&header)[0].1;
            assert_eq!(
                parse_accept_encoding_with_quality(&header)[0].1,
                u16::from(legacy) * 10,
                "{value}"
            );
        }
    }

    #[test]
    fn percentage_helper_keeps_its_existing_return_values() {
        let parsed: Vec<(String, u8)> = parse_accept_encoding("gzip;q=0.001, br;q=0.109");
        assert_eq!(parsed, [("br".to_owned(), 10), ("gzip".to_owned(), 0)]);
    }
}
