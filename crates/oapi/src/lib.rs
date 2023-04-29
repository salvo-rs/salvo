#![doc = include_str!("../docs/lib.md")]
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::future_not_send)]
#![warn(rustdoc::broken_intra_doc_links)]

#[macro_use]
mod cfg;

mod openapi;
pub use openapi::*;

mod endpoint;
pub use endpoint::{Endpoint, EndpointModifier, EndpointRegistry};
pub mod extract;
mod router;

cfg_feature! {
    #![feature ="swagger-ui"]
    pub mod swagger_ui;
}

#[doc = include_str!("../docs/endpoint.md")]
pub use salvo_oapi_macros::endpoint;
#[doc = include_str!("../docs/schema.md")]
pub use salvo_oapi_macros::schema;
#[doc = include_str!("../docs/derive_to_parameters.md")]
pub use salvo_oapi_macros::ToParameters;
#[doc = include_str!("../docs/derive_to_response.md")]
pub use salvo_oapi_macros::ToResponse;
#[doc = include_str!("../docs/derive_to_responses.md")]
pub use salvo_oapi_macros::ToResponses;
#[doc = include_str!("../docs/derive_to_schema.md")]
pub use salvo_oapi_macros::ToSchema;

use std::collections::{BTreeMap, HashMap};

use salvo_core::extract::Extractible;

// https://github.com/bkchr/proc-macro-crate/issues/10
extern crate self as salvo_oapi;

/// Trait for implementing OpenAPI Schema object.
///
/// Generated schemas can be referenced or reused in path operations.
///
/// This trait is derivable and can be used with `[#derive]` attribute. For a details of
/// `#[derive(ToSchema)]` refer to [derive documentation][derive].
///
/// [derive]: derive.ToSchema.html
///
/// # Examples
///
/// Use `#[derive]` to implement `ToSchema` trait.
/// ```
/// use salvo_oapi::ToSchema;
/// #[derive(ToSchema)]
/// #[schema(example = json!({"name": "bob the cat", "id": 1}))]
/// struct Pet {
///     id: u64,
///     name: String,
///     age: Option<i32>,
/// }
/// ```
///
/// Following manual implementation is equal to above derive one.
/// ```
/// use salvo_oapi::{ToSchema, RefOr, Schema, SchemaFormat, SchemaType, KnownFormat, Object};
/// # struct Pet {
/// #     id: u64,
/// #     name: String,
/// #     age: Option<i32>,
/// # }
/// #
/// impl ToSchema for Pet {
///     fn to_schema() -> (Option<String>, RefOr<Schema>) {
///         (None, Object::new()
///             .property(
///                 "id",
///                 Object::new()
///                     .schema_type(SchemaType::Integer)
///                     .format(SchemaFormat::KnownFormat(
///                         KnownFormat::Int64,
///                     )),
///             )
///             .required("id")
///             .property(
///                 "name",
///                 Object::new()
///                     .schema_type(SchemaType::String),
///             )
///             .required("name")
///             .property(
///                 "age",
///                 Object::new()
///                     .schema_type(SchemaType::Integer)
///                     .format(SchemaFormat::KnownFormat(
///                         KnownFormat::Int32,
///                     )),
///             )
///             .example(serde_json::json!({
///               "name":"bob the cat","id":1
///             }))
///             .into()
///         )
///     }
/// }
/// ```
pub trait ToSchema {
    /// Returns a tuple of name and schema or reference to a schema that can be referenced by the
    /// name or inlined directly to responses, request bodies or parameters.
    fn to_schema() -> (Option<String>, RefOr<schema::Schema>);
}

impl<T: ToSchema> From<T> for RefOr<schema::Schema> {
    fn from(_: T) -> Self {
        T::to_schema().1
    }
}

/// Represents _`nullable`_ type. This can be used anywhere where "nothing" needs to be evaluated.
/// This will serialize to _`null`_ in JSON and [`schema::empty`] is used to create the
/// [`schema::Schema`] for the type.
pub type TupleUnit = ();

impl ToSchema for TupleUnit {
    fn to_schema() -> (Option<String>, RefOr<schema::Schema>) {
        (Some("TupleUnit".into()), schema::empty().into())
    }
}

macro_rules! impl_to_schema {
    ( $ty:path ) => {
        impl_to_schema!( @impl_schema $ty );
    };
    ( & $ty:path ) => {
        impl_to_schema!( @impl_schema &$ty );
    };
    ( @impl_schema $( $tt:tt )* ) => {
        impl ToSchema for $($tt)* {
            fn to_schema() -> (Option<String>, crate::RefOr<crate::schema::Schema>) {
                (None, schema!( $($tt)* ).into())
            }
        }
    };
}

macro_rules! impl_to_schema_primitive {
    ( $( $tt:path  ),* ) => {
        $( impl_to_schema!( $tt ); )*
    };
}

// Create `salvo-oapi` module so we can use `salvo-oapi-macros` directly
// from `salvo-oapi` crate. ONLY FOR INTERNAL USE!
#[doc(hidden)]
pub mod oapi {
    pub use super::*;
}
// Create `salvo-oapi` module so we can use `salvo-oapi-macros` directly
// from `salvo-oapi` crate. ONLY FOR INTERNAL USE!
#[doc(hidden)]
pub mod __private {
    pub use inventory;
}

#[rustfmt::skip]
impl_to_schema_primitive!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, bool, f32, f64, String, str, char
);
impl_to_schema!(&str);

impl<T: ToSchema> ToSchema for Vec<T> {
    fn to_schema() -> (Option<String>, RefOr<schema::Schema>) {
        (None, schema!(#[inline] Vec<T>).into())
    }
}

impl<T: ToSchema> ToSchema for [T] {
    fn to_schema() -> (Option<String>, RefOr<schema::Schema>) {
        (
            None,
            schema!(
                #[inline]
                [T]
            )
            .into(),
        )
    }
}

impl<T: ToSchema> ToSchema for &[T] {
    fn to_schema() -> (Option<String>, RefOr<schema::Schema>) {
        (None, schema!(#[inline]&[T]).into())
    }
}

impl<T: ToSchema> ToSchema for Option<T> {
    fn to_schema() -> (Option<String>, RefOr<schema::Schema>) {
        (None, schema!(#[inline] Option<T>).into())
    }
}

impl<K: ToSchema, V: ToSchema> ToSchema for BTreeMap<K, V> {
    fn to_schema() -> (Option<String>, RefOr<schema::Schema>) {
        (None, schema!(#[inline]BTreeMap<K, V>).into())
    }
}

impl<K: ToSchema, V: ToSchema> ToSchema for HashMap<K, V> {
    fn to_schema() -> (Option<String>, RefOr<schema::Schema>) {
        (None, schema!(#[inline]HashMap<K, V>).into())
    }
}

/// Trait used to convert implementing type to OpenAPI parameters.
///
/// This trait is [derivable][derive] for structs which are used to describe `path` or `query` parameters.
/// For more details of `#[derive(ToParameters)]` refer to [derive documentation][derive].
///
/// # Examples
///
/// Derive [`ToParameters`] implementation. This example will fail to compile because [`ToParameters`] cannot
/// be used alone and it need to be used together with endpoint using the params as well. See
/// [derive documentation][derive] for more details.
/// ```
/// use serde::Deserialize;
/// use salvo_oapi::{ToParameters, EndpointModifier, Components, Operation};
/// use salvo_core::prelude::*;
///
/// #[derive(Deserialize, ToParameters)]
/// struct PetParams {
///     /// Id of pet
///     id: i64,
///     /// Name of pet
///     name: String,
/// }
/// ```
///
/// Roughly equal manual implementation of [`ToParameters`] trait.
/// ```
/// # use serde::Deserialize;
/// # use salvo_oapi::{ToParameters, EndpointModifier, Components, Operation};
/// # use salvo_core::prelude::*;
/// # use salvo_core::extract::{Metadata, Extractible};
/// #[derive(Deserialize)]
/// # struct PetParams {
/// #    /// Id of pet
/// #    id: i64,
/// #    /// Name of pet
/// #    name: String,
/// # }
/// impl<'de> salvo_oapi::ToParameters<'de> for PetParams {
///     fn to_parameters() -> salvo_oapi::Parameters {
///         salvo_oapi::Parameters::new().parameter(
///             salvo_oapi::Parameter::new("id")
///                 .required(salvo_oapi::Required::True)
///                 .parameter_in(salvo_oapi::ParameterIn::Path)
///                 .description("Id of pet")
///                 .schema(
///                     salvo_oapi::Object::new()
///                         .schema_type(salvo_oapi::SchemaType::Integer)
///                         .format(salvo_oapi::SchemaFormat::KnownFormat(salvo_oapi::schema::KnownFormat::Int64)),
///                 ),
///         ).parameter(
///             salvo_oapi::Parameter::new("name")
///                 .required(salvo_oapi::Required::True)
///                 .parameter_in(salvo_oapi::ParameterIn::Query)
///                 .description("Name of pet")
///                 .schema(
///                     salvo_oapi::Object::new()
///                         .schema_type(salvo_oapi::SchemaType::String),
///                 ),
///         )
///     }
/// }
///
/// #[async_trait]
/// impl<'de> Extractible<'de> for PetParams {
///    fn metadata() -> &'de Metadata {
///      static METADATA: Metadata = Metadata::new("");
///      &METADATA
///    }
///    async fn extract(req: &'de mut Request) -> Result<Self, salvo_core::http::ParseError> {
///        salvo_core::serde::from_request(req, Self::metadata()).await
///    }
///    async fn extract_with_arg(req: &'de mut Request, _arg: &str) -> Result<Self, salvo_core::http::ParseError> {
///        Self::extract(req).await
///    }
/// }
///
/// #[async_trait]
/// impl EndpointModifier for PetParams {
///     fn modify(_components: &mut Components, operation: &mut Operation) {
///         operation.parameters.append(&mut PetParams::to_parameters());
///     }
/// }
/// ```
/// [derive]: derive.ToParameters.html
pub trait ToParameters<'de>: Extractible<'de> + EndpointModifier {
    /// Provide [`Vec`] of [`Parameter`]s to caller. The result is used in `salvo-oapi-macros` library to
    /// provide OpenAPI parameter information for the endpoint using the parameters.
    fn to_parameters() -> Parameters;
}

/// Trait used to give [`Parameter`] information for OpenAPI.
pub trait ToParameter: EndpointModifier {
    /// Returns a `Parameter`.
    fn to_parameter() -> Parameter;
    /// Returns a `Parameter`, this is used internal.
    fn to_parameter_with_arg(_arg: &str) -> Parameter {
        Self::to_parameter()
    }
}

/// This trait is implemented to document a type (like an enum) which can represent
/// request body, to be used in operation.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use serde::Deserialize;
/// use salvo_oapi::{ToRequestBody, ToSchema, Components, Content, EndpointModifier, Operation, RequestBody };
///
/// #[derive(ToSchema, Deserialize, Debug)]
/// struct MyPayload {
///     name: String,
/// }
///
/// impl ToRequestBody for MyPayload {
///     fn to_request_body() -> RequestBody {
///         RequestBody::new()
///             .add_content("application/json", Content::new(MyPayload::to_schema().1))
///     }
/// }
/// impl EndpointModifier for MyPayload {
///     fn modify(_components: &mut Components, operation: &mut Operation) {
///         operation.request_body = Some(Self::to_request_body());
///     }
/// }
/// ```
pub trait ToRequestBody: EndpointModifier {
    /// Returns `RequestBody`.
    fn to_request_body() -> RequestBody;
}

/// This trait is implemented to document a type (like an enum) which can represent multiple
/// responses, to be used in operation.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use salvo_oapi::{Response, Responses, RefOr, ToResponses };
///
/// enum MyResponse {
///     Ok,
///     NotFound,
/// }
///
/// impl ToResponses for MyResponse {
///     fn to_responses() -> Responses {
///         Responses::new()
///             .response("200", Response::new("Ok"))
///             .response("404", Response::new("Not Found"))
///     }
/// }
/// ```
pub trait ToResponses {
    /// Returns an ordered map of response codes to responses.
    fn to_responses() -> Responses;
}

/// This trait is implemented to document a type which represents a single response which can be
/// referenced or reused as a component in multiple operations.
///
/// _`ToResponse`_ trait can also be derived with [`#[derive(ToResponse)]`][derive].
///
/// # Examples
///
/// ```
/// use salvo_oapi::{RefOr, Response, ToResponse};
///
/// struct MyResponse;
/// impl ToResponse for MyResponse {
///     fn to_response() -> (String, RefOr<Response>) {
///         (
///             "MyResponse".into(),
///             Response::new("My Response").into(),
///         )
/// }
///     }
/// ```
///
/// [derive]: derive.ToResponse.html
pub trait ToResponse {
    /// Returns a tuple of response component name (to be referenced) to a response.
    fn to_response() -> (String, RefOr<crate::Response>);
}

impl<T> ToResponses for T where T: ToResponse {
    fn to_responses() -> Responses {
        let (key, response) = T::to_response();
        Responses::new().response(key, response)
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_primitive_schema() {
        for (name, schema, value) in [
            ("i8", i8::to_schema().1, json!({"type": "integer", "format": "int32"})),
            ("i16", i16::to_schema().1, json!({"type": "integer", "format": "int32"})),
            ("i32", i32::to_schema().1, json!({"type": "integer", "format": "int32"})),
            ("i64", i64::to_schema().1, json!({"type": "integer", "format": "int64"})),
            ("i128", i128::to_schema().1, json!({"type": "integer"})),
            ("isize", isize::to_schema().1, json!({"type": "integer"})),
            (
                "u8",
                u8::to_schema().1,
                json!({"type": "integer", "format": "int32", "minimum": 0.0}),
            ),
            (
                "u16",
                u16::to_schema().1,
                json!({"type": "integer", "format": "int32", "minimum": 0.0}),
            ),
            (
                "u32",
                u32::to_schema().1,
                json!({"type": "integer", "format": "int32", "minimum": 0.0}),
            ),
            (
                "u64",
                u64::to_schema().1,
                json!({"type": "integer", "format": "int64", "minimum": 0.0}),
            ),
            ("u128", u128::to_schema().1, json!({"type": "integer", "minimum": 0.0})),
            (
                "usize",
                usize::to_schema().1,
                json!({"type": "integer", "minimum": 0.0 }),
            ),
            ("bool", bool::to_schema().1, json!({"type": "boolean"})),
            ("str", str::to_schema().1, json!({"type": "string"})),
            ("String", String::to_schema().1, json!({"type": "string"})),
            ("char", char::to_schema().1, json!({"type": "string"})),
            ("f32", f32::to_schema().1, json!({"type": "number", "format": "float"})),
            ("f64", f64::to_schema().1, json!({"type": "number", "format": "double"})),
        ] {
            println!("{name}: {json}", json = serde_json::to_string(&schema).unwrap());
            let schema = serde_json::to_value(schema).unwrap();
            assert_json_eq!(schema, value);
        }
    }
}
