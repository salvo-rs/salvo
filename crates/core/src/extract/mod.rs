//! Extract is a feature to let you deserialize request to custom type.
//!
//! You can easily get data from multiple different data sources and assemble it into the type you want.
//! You can define a custom type first, then in `Handler` you can get the data like this:
//!
//! ```
//! # use salvo_core::prelude::*;
//! # use serde::{Deserialize, Serialize};
//! #[derive(Serialize, Deserialize, Extractible, Debug)]
//! // Get the data field value from the body by default.
//! #[extract(default_source(from = "body"))]
//! struct GoodMan<'a> {
//!     /// The id number is obtained from the request path parameter, and the data is automatically parsed as i64 type.
//!     #[extract(source(from = "param"))]
//!     id: i64,
//!     /// Reference types can be used to avoid memory copying.
//!     username: &'a str,
//!     first_name: String,
//!     last_name: String,
//! }
//!
//! #[handler]
//! async fn edit(req: &mut Request) {
//!     let good_man: GoodMan<'_> = req.extract().await.unwrap();
//! }
//! ```
//!
//! There is considerable flexibility in the definition of data types, and can even be resolved into nested structures as needed:
//!
//! ```
//! # use salvo_core::prelude::*;
//! # use serde::{Deserialize, Serialize};
//! #[derive(Serialize, Deserialize, Extractible, Debug)]
//! #[extract(default_source(from = "body", format = "json"))]
//! struct GoodMan<'a> {
//!     #[extract(source(from = "param"))]
//!     id: i64,
//!     #[extract(source(from = "query"))]
//!     username: &'a str,
//!     first_name: String,
//!     last_name: String,
//!     lovers: Vec<String>,
//!     /// The nested field is completely reparsed from Request.
//!     #[extract(source(from = "request"))]
//!     nested: Nested<'a>,
//! }
//!
//! #[derive(Serialize, Deserialize, Extractible, Debug)]
//! #[extract(default_source(from = "body", format = "json"))]
//! struct Nested<'a> {
//!     #[extract(source(from = "param"))]
//!     id: i64,
//!     #[extract(source(from = "query"))]
//!     username: &'a str,
//!     first_name: String,
//!     last_name: String,
//!     #[extract(rename = "lovers")]
//!     #[serde(default)]
//!     pets: Vec<String>,
//! }
//! ```
//!
//! View [full source code](https://github.com/salvo-rs/salvo/blob/main/examples/extract-nested/src/main.rs)

/// Metadata types.
pub mod metadata;
pub use metadata::Metadata;

use async_trait::async_trait;
use serde::Deserialize;

use crate::http::{ParseError, Request};
use crate::serde::from_request;

/// If a type implements this trait, it will give a metadata, this will help request to extracts data to this type.
#[async_trait]
pub trait Extractible<'de>: Deserialize<'de> {
    /// Metadata for Extractible type.
    fn metadata() -> &'de Metadata;

    /// Extract data from request.
    async fn extract(req: &'de mut Request) -> Result<Self, ParseError> {
        from_request(req, Self::metadata()).await
    }
    /// Extract data from request with a argument. This function used in macros internal.
    async fn extract_with_arg(req: &'de mut Request, _arg: &str) -> Result<Self, ParseError> {
        Self::extract(req).await
    }
}
