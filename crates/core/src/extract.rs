//! Request extraction for deserializing request data into application types.
//!
//! Extraction can collect values from multiple request sources, such as path
//! parameters, query strings, headers, cookies, form bodies, and JSON bodies.
//! Define an [`Extractible`] type, then ask the request to build it in a handler:
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
//!     let _ = good_man;
//! }
//! ```
//!
//! Extracted types can be nested. Use `#[salvo(extract(flatten))]` when a nested
//! extractible type should be parsed from the same request. (`#[serde(flatten)]`
//! is not supported on `#[derive(Extractible)]` fields — it conflicts with
//! Salvo's extraction and is rejected at compile time; use the attribute below.)
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
//!     /// The nested field is parsed from the same request.
//!     #[salvo(extract(flatten))]
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
//! View the [full nested extraction example] in the repository.
//!
//! [full nested extraction example]: https://github.com/salvo-rs/salvo/blob/main/examples/extract-nested/src/main.rs

/// Metadata types.
pub mod metadata;
pub use metadata::Metadata;
mod parameter;
#[cfg(feature = "cookie")]
pub use parameter::CookieParam;
pub use parameter::{HeaderParam, PathParam, QueryParam};
mod payload;
pub use payload::{FormBody, FormFile, FormFiles, JsonBody};
mod case;
use std::fmt::Debug;

pub use case::RenameRule;

use crate::http::{ParseError, Request};
use crate::{Depot, Writer};

/// Describes how to extract a type from a request.
///
/// Implementations provide metadata used by Salvo's extraction machinery and
/// OpenAPI integration, then perform the actual extraction from [`Request`] and
/// [`Depot`].
pub trait Extractible<'ex> {
    /// Metadata for this extractible type.
    fn metadata() -> &'static Metadata;

    /// Extracts data from a request.
    ///
    /// **Note:** if extraction fails and the response does not already contain
    /// an error status, Salvo renders the failure as `400 Bad Request`.
    fn extract(
        req: &'ex mut Request,
        depot: &'ex mut Depot,
    ) -> impl Future<Output = Result<Self, impl Writer + Send + Debug + 'static>> + Send
    where
        Self: Sized;

    /// Extract data from a request with an argument. This function is used internally by macros.
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
        Ok(T::extract(req, depot).await.ok())
    }
    #[allow(refining_impl_trait)]
    fn extract_with_arg(
        req: &'ex mut Request,
        depot: &'ex mut Depot,
        arg: &str,
    ) -> impl Future<Output = Result<Self, ParseError>> + Send {
        futures_util::FutureExt::map(T::extract_with_arg(req, depot, arg), |result| {
            Ok(result.ok())
        })
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use crate::extract::{HeaderParam, PathParam, QueryParam};
    use crate::prelude::*;
    use crate::test::{ResponseExt, TestClient};

    #[handler]
    async fn optional_parameters(
        id: Option<PathParam<u32>>,
        page: Option<QueryParam<u32>>,
        limit: Option<HeaderParam<u32>>,
    ) -> String {
        format!(
            "{:?}/{:?}/{:?}",
            id.map(PathParam::into_inner),
            page.map(QueryParam::<u32>::into_inner),
            limit.map(HeaderParam::<u32>::into_inner),
        )
    }

    #[tokio::test]
    async fn optional_named_parameters_preserve_argument_names() {
        let service = Service::new(Router::with_path("{id}").get(optional_parameters));
        let mut response = TestClient::get("http://localhost/7?page=2")
            .add_header("limit", "5", true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(
            response.take_string().await.unwrap(),
            "Some(7)/Some(2)/Some(5)"
        );

        let mut response = TestClient::get("http://localhost/7").send(&service).await;
        assert_eq!(response.take_string().await.unwrap(), "Some(7)/None/None");

        let mut response = TestClient::get("http://localhost/invalid?page=invalid")
            .add_header("limit", "invalid", true)
            .send(&service)
            .await;
        assert_eq!(response.take_string().await.unwrap(), "None/None/None");

        let mut response = TestClient::get("http://localhost/")
            .send(Router::new().get(optional_parameters))
            .await;
        assert_eq!(response.take_string().await.unwrap(), "None/None/None");
    }

    #[cfg(feature = "cookie")]
    #[tokio::test]
    async fn optional_cookie_parameter_preserves_argument_name() {
        use crate::extract::CookieParam;

        #[handler]
        async fn optional_cookie(count: Option<CookieParam<u32>>) -> String {
            format!("{:?}", count.map(CookieParam::<u32>::into_inner))
        }

        let service = Service::new(Router::new().get(optional_cookie));
        for (cookie, expected) in [
            ("count=3", "Some(3)"),
            ("count=invalid", "None"),
            ("other=3", "None"),
        ] {
            let mut response = TestClient::get("http://localhost/")
                .add_header("cookie", cookie, true)
                .send(&service)
                .await;
            assert_eq!(response.take_string().await.unwrap(), expected);
        }
    }

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
