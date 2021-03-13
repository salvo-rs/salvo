// #![feature(specialization)]
// #![feature(proc_macro_hygiene)]
// #![feature(crate_visibility_modifier)]
// #![feature(doc_cfg)]
// #![recursion_limit="512"]

//! Types that map to concepts in HTTP.
//!
//! This module exports types that map to HTTP concepts or to the underlying
//! HTTP library when needed. Because the underlying HTTP library is likely to
//! change (see [#17]), types in [`hyper`] should be considered unstable.
//!
//! [#17]: https://github.com/SergioBenitez/Novel/issues/17

pub mod body_chunk;
pub mod errors;
pub mod form;
pub mod multipart;
pub mod range;
pub mod request;
pub mod response;

pub use body_chunk::BodyChunk;
pub use cookie;
pub use http::method::Method;
pub use http::{HeaderMap, HeaderValue, StatusCode};
pub use mime::Mime;
pub use range::HttpRange;
pub use request::Request;
pub use response::Response;
pub use errors::{HttpError, ReadError};

pub use http::header;
pub use headers;

pub fn guess_accept_mime(req: &Request, default_type: Option<Mime>) -> Mime {
    let dmime: Mime = default_type.unwrap_or_else(|| "text/html".parse().unwrap());
    let accept = req.accept();
    accept.first().unwrap_or(&dmime).to_string().parse().unwrap_or(dmime)
}
