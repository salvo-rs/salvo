//! HTTP Errors.

mod parse_error;
cfg_feature! {
    #![feature = "rfc9457"]
    mod problem;
    pub use problem::{NoExtensions, PROBLEM_JSON, PlainProblem, Problem};
}
mod status_error;
pub use parse_error::{ParseError, ParseResult};
pub use status_error::{StatusError, StatusResult};
