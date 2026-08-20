//! Request body extractors.
mod file;
mod form;
mod json;

pub use file::{FormFile, FormFiles};
pub use form::FormBody;
pub use json::JsonBody;
