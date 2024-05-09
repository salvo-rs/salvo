//! Request body extractors for the API operation.
mod form;
mod json;
mod file;

pub use form::FormBody;
pub use json::JsonBody;
pub use file::FormFile;
