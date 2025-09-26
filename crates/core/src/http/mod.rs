//! The HTTP related types and functions.

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
pub use mime::Mime;
pub use response::Response;

pub use http::version::Version;

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
