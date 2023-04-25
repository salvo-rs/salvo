//! Request body extractors for the API operation.
mod form;
mod json;

pub use form::FormBody;
pub use json::JsonBody;
