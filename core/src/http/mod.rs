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

extern crate cookie;
extern crate time;
extern crate state;
mod request;
mod response;
pub mod form;
pub mod headers;

pub use request::Request;
pub use response::{Response, BodyWriter};
pub use http::{method::Method, StatusCode};
pub use mime::Mime;
pub use hyper::Body;
