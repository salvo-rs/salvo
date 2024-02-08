//! HTTP Errors.

mod parse_error;
mod status_error;
pub use parse_error::{ParseError, ParseResult};
pub use status_error::{StatusError, StatusResult};
