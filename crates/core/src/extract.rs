//! Extract is a feature to let you deserialize request to custom type.
//!
//! You can easily get data from multiple different data sources and assemble it into the type you
//! want. You can define a custom type first, then in `Handler` you can get the data like this:
//!
//! ```
//! # use salvo_core::prelude::*;
//! # use serde::{Deserialize, Serialize};
//! #[derive(Serialize, Deserialize, Extractible, Debug)]
//! // Get the data field value from the body by default.
//! #[salvo(extract(default_source(from = "body")))]
//! struct GoodMan<'a> {
//!     /// The id number is obtained from the request path parameter, and the data is automatically parsed as i64 type.
//!     #[salvo(extract(source(from = "param")))]
//!     id: i64,
//!     /// Reference types can be used to avoid memory copying.
//!     username: &'a str,
//!     first_name: String,
//!     last_name: String,
//! }
//!
//! #[handler]
//! async fn edit(req: &mut Request, depot: &mut Depot) {
//!     let good_man: GoodMan<'_> = req.extract(depot).await.unwrap();
//! }
//! ```
//!
//! There is considerable flexibility in the definition of data types, and can even be resolved into
//! nested structures as needed:
//!
//! ```
//! # use salvo_core::prelude::*;
//! # use serde::{Deserialize, Serialize};
//! #[derive(Serialize, Deserialize, Extractible, Debug)]
//! #[salvo(extract(default_source(from = "body")))]
//! struct GoodMan<'a> {
//!     #[salvo(extract(source(from = "param")))]
//!     id: i64,
//!     #[salvo(extract(source(from = "query")))]
//!     username: &'a str,
//!     first_name: String,
//!     last_name: String,
//!     lovers: Vec<String>,
//!     /// The nested field is completely reparsed from Request.
//!     #[serde(flatten)]
//!     nested: Nested<'a>,
//! }
//!
//! #[derive(Serialize, Deserialize, Extractible, Debug)]
//! #[salvo(extract(default_source(from = "body")))]
//! struct Nested<'a> {
//!     #[salvo(extract(source(from = "param")))]
//!     id: i64,
//!     #[salvo(extract(source(from = "query")))]
//!     username: &'a str,
//!     first_name: String,
//!     last_name: String,
//!     #[salvo(rename = "lovers")]
//!     #[serde(default)]
//!     pets: Vec<String>,
//! }
//! ```
//!
//! View [full source code](https://github.com/salvo-rs/salvo/blob/main/examples/extract-nested/src/main.rs)

/// Metadata types.
pub mod metadata;
pub use metadata::Metadata;
mod case;
use std::fmt::Debug;

pub use case::RenameRule;
use futures_util::FutureExt;

use crate::http::{ParseError, Request};
use crate::{Depot, Writer};

/// If a type implements this trait, it will give a metadata, this will help request to extracts
/// data to this type.
pub trait Extractible<'ex> {
    /// Metadata for Extractible type.
    fn metadata() -> &'static Metadata;

    /// Extract data from request.
    ///
    /// **NOTE:** Set status code to 400 if extract failed and status code is not error.
    fn extract(
        req: &'ex mut Request,
        depot: &'ex mut Depot,
    ) -> impl Future<Output = Result<Self, impl Writer + Send + Debug + 'static>> + Send
    where
        Self: Sized;

    /// Extract data from request with a argument. This function used in macros internal.
    fn extract_with_arg(
        req: &'ex mut Request,
        depot: &'ex mut Depot,
        _arg: &str,
    ) -> impl Future<Output = Result<Self, impl Writer + Send + Debug + 'static>> + Send
    where
        Self: Sized,
    {
        Self::extract(req, depot)
    }
}

impl<'ex, T> Extractible<'ex> for Option<T>
where
    T: Extractible<'ex> + ::serde::de::Deserialize<'ex>,
{
    fn metadata() -> &'static Metadata {
        T::metadata()
    }
    #[allow(refining_impl_trait)]
    async fn extract(req: &'ex mut Request, depot: &'ex mut Depot) -> Result<Self, ParseError> {
        Ok(T::extract(req, depot).boxed().await.ok())
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use crate::prelude::*;
    use crate::test::TestClient;

    #[tokio::test]
    async fn test_generic_struct() {
        #[derive(Debug, Deserialize, Extractible)]
        struct Outer<T> {
            #[salvo(extract(flatten))]
            inner: T,
        }
        #[derive(Debug, Deserialize, Extractible)]
        #[salvo(extract(default_source(from = "query")))]
        struct Inner {
            a: String,
        }
        let mut req = TestClient::get("http://127.0.0.1:8698/test/1234/param2v")
            .query("a", "1")
            .build();
        let mut depot = crate::Depot::new();
        let data: Outer<Inner> = req.extract(&mut depot).await.unwrap();
        assert_eq!(data.inner.a, "1");
    }
}
