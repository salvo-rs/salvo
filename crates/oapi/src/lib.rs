#![cfg_attr(doc_cfg, feature(doc_cfg))]

mod openapi;
pub use openapi::*;
mod endpoint;
pub use endpoint::{Endpoint, EndpointModifier, EndpointRegistry};
pub mod extract;
mod router;
pub mod swagger;

use salvo_core::Extractible;
pub use salvo_oapi_macros::*;
use std::collections::{BTreeMap, HashMap};

// https://github.com/bkchr/proc-macro-crate/issues/10
extern crate self as salvo_oapi;

/// Trait for implementing OpenAPI Schema object.
///
/// Generated schemas can be referenced or reused in path operations.
///
/// This trait is derivable and can be used with `[#derive]` attribute. For a details of
/// `#[derive(AsSchema)]` refer to [derive documentation][derive].
///
/// [derive]: derive.AsSchema.html
///
/// # Examples
///
/// Use `#[derive]` to implement `AsSchema` trait.
/// ```
/// use salvo_oapi::AsSchema;
/// #[derive(AsSchema)]
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
/// use salvo_oapi::{AsSchema, RefOr, Schema, SchemaFormat, SchemaType, KnownFormat, Object};
/// # struct Pet {
/// #     id: u64,
/// #     name: String,
/// #     age: Option<i32>,
/// # }
/// #
/// impl AsSchema for Pet {
///     fn schema() -> (Option<&'static str>, RefOr<Schema>) {
///          (
///             Some("Pet"),
///             Object::new()
///                 .property(
///                     "id",
///                     Object::new()
///                         .schema_type(SchemaType::Integer)
///                         .format(SchemaFormat::KnownFormat(
///                             KnownFormat::Int64,
///                         )),
///                 )
///                 .required("id")
///                 .property(
///                     "name",
///                     Object::new()
///                         .schema_type(SchemaType::String),
///                 )
///                 .required("name")
///                 .property(
///                     "age",
///                     Object::new()
///                         .schema_type(SchemaType::Integer)
///                         .format(SchemaFormat::KnownFormat(
///                             KnownFormat::Int32,
///                         )),
///                 )
///                 .example(serde_json::json!({
///                   "name":"bob the cat","id":1
///                 }))
///                 .into(),
///         ) }
/// }
/// ```
pub trait AsSchema {
    /// Return a tuple of name and schema or reference to a schema that can be referenced by the
    /// name or inlined directly to responses, request bodies or parameters.
    fn schema() -> (Option<&'static str>, RefOr<schema::Schema>);

    /// Optional set of alias schemas for the [`AsSchema::schema`].
    ///
    /// Typically there is no need to manually implement this method but it is instead implemented
    /// by derive [`macro@AsSchema`] when `#[aliases(...)]` attribute is defined.
    fn aliases() -> Vec<(&'static str, schema::Schema)> {
        Vec::new()
    }
}

impl<T: AsSchema> From<T> for RefOr<schema::Schema> {
    fn from(_: T) -> Self {
        T::schema().1
    }
}

/// Represents _`nullable`_ type. This can be used anywhere where "nothing" needs to be evaluated.
/// This will serialize to _`null`_ in JSON and [`schema::empty`] is used to create the
/// [`schema::Schema`] for the type.
pub type TupleUnit = ();

impl AsSchema for TupleUnit {
    fn schema() -> (Option<&'static str>, RefOr<schema::Schema>) {
        (Some("TupleUnit"), schema::empty().into())
    }
}

macro_rules! impl_partial_schema {
    ( $ty:path ) => {
        impl_partial_schema!( @impl_schema $ty );
    };
    ( & $ty:path ) => {
        impl_partial_schema!( @impl_schema &$ty );
    };
    ( @impl_schema $( $tt:tt )* ) => {
        impl AsSchema for $($tt)* {
            fn schema() -> (Option<&'static str>, crate::RefOr<crate::schema::Schema>) {
                (None, schema!( $($tt)* ).into())
            }
        }
    };
}

macro_rules! impl_partial_schema_primitive {
    ( $( $tt:path  ),* ) => {
        $( impl_partial_schema!( $tt ); )*
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
impl_partial_schema_primitive!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, bool, f32, f64, String, str, char
);

impl_partial_schema!(&str);

impl<T: AsSchema> AsSchema for Vec<T> {
    fn schema() -> (Option<&'static str>, RefOr<schema::Schema>) {
        (None, schema!(#[inline] Vec<T>).into())
    }
}

impl<T: AsSchema> AsSchema for [T] {
    fn schema() -> (Option<&'static str>, RefOr<schema::Schema>) {
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

impl<T: AsSchema> AsSchema for &[T] {
    fn schema() -> (Option<&'static str>, RefOr<schema::Schema>) {
        (
            None,
            schema!(
                #[inline]
                &[T]
            )
            .into(),
        )
    }
}

impl<T: AsSchema> AsSchema for Option<T> {
    fn schema() -> (Option<&'static str>, RefOr<schema::Schema>) {
        (None, schema!(#[inline] Option<T>).into())
    }
}

impl<K: AsSchema, V: AsSchema> AsSchema for BTreeMap<K, V> {
    fn schema() -> (Option<&'static str>, RefOr<schema::Schema>) {
        (None, schema!(#[inline]BTreeMap<K, V>).into())
    }
}

impl<K: AsSchema, V: AsSchema> AsSchema for HashMap<K, V> {
    fn schema() -> (Option<&'static str>, RefOr<schema::Schema>) {
        (None, schema!(#[inline]HashMap<K, V>).into())
    }
}

/// Trait used to convert implementing type to OpenAPI parameters.
///
/// This trait is [derivable][derive] for structs which are used to describe `path` or `query` parameters.
/// For more details of `#[derive(AsParameters)]` refer to [derive documentation][derive].
///
/// # Examples
///
/// Derive [`AsParameters`] implementation. This example will fail to compile because [`AsParameters`] cannot
/// be used alone and it need to be used together with endpoint using the params as well. See
/// [derive documentation][derive] for more details.
/// ```
/// use serde::Deserialize;
/// use salvo_oapi::{AsParameters, EndpointModifier, Components, Operation};
/// use salvo_core::prelude::*;
///
/// #[derive(Deserialize, AsParameters)]
/// struct PetParams {
///     /// Id of pet
///     id: i64,
///     /// Name of pet
///     name: String,
/// }
/// ```
///
/// Roughly equal manual implementation of [`AsParameters`] trait.
/// ```
/// # use serde::Deserialize;
/// # use salvo_oapi::{AsParameters, EndpointModifier, Components, Operation};
/// # use salvo_core::prelude::*;
/// # use salvo_core::extract::{Metadata, Extractible};
/// #[derive(Deserialize)]
/// # struct PetParams {
/// #    /// Id of pet
/// #    id: i64,
/// #    /// Name of pet
/// #    name: String,
/// # }
/// impl<'de> salvo_oapi::AsParameters<'de> for PetParams {
///     fn parameters() -> salvo_oapi::Parameters {
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
///         operation.parameters.append(&mut PetParams::parameters());
///     }
/// }
/// ```
/// [derive]: derive.AsParameters.html
pub trait AsParameters<'de>: Extractible<'de> + EndpointModifier {
    /// Provide [`Vec`] of [`Parameter`]s to caller. The result is used in `salvo-oapi-macros` library to
    /// provide OpenAPI parameter information for the endpoint using the parameters.
    fn parameters() -> Parameters;
}
pub trait AsParameter: EndpointModifier {
    fn parameter() -> Parameter;
    fn parameter_with_arg(_arg: &str) -> Parameter {
        Self::parameter()
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
/// use salvo_oapi::{AsRequestBody, AsSchema, Components, Content, EndpointModifier, Operation, RequestBody };
///
/// #[derive(AsSchema, Deserialize, Debug)]
/// struct MyPayload {
///     name: String,
/// }
///
/// impl AsRequestBody for MyPayload {
///     fn request_body() -> RequestBody {
///         RequestBody::new()
///             .add_content("application/json", Content::new(MyPayload::schema().1))
///     }
/// }
/// impl EndpointModifier for MyPayload {
///     fn modify(_components: &mut Components, operation: &mut Operation) {
///         operation.request_body = Some(Self::request_body());
///     }
/// }
/// ```
pub trait AsRequestBody: EndpointModifier {
    /// Returns `RequestBody`.
    fn request_body() -> RequestBody;
}

/// This trait is implemented to document a type (like an enum) which can represent multiple
/// responses, to be used in operation.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use salvo_oapi::{Response, Responses, RefOr, AsResponses };
///
/// enum MyResponse {
///     Ok,
///     NotFound,
/// }
///
/// impl AsResponses for MyResponse {
///     fn responses() -> Responses {
///         Responses::new()
///             .response("200", Response::new("Ok"))
///             .response("404", Response::new("Not Found"))
///     }
/// }
/// ```
pub trait AsResponses {
    /// Returns an ordered map of response codes to responses.
    fn responses() -> Responses;
}

/// This trait is implemented to document a type which represents a single response which can be
/// referenced or reused as a component in multiple operations.
///
/// _`AsResponse`_ trait can also be derived with [`#[derive(AsResponse)]`][derive].
///
/// # Examples
///
/// ```
/// use salvo_oapi::{RefOr, Response, AsResponse};
///
/// struct MyResponse;
/// impl AsResponse for MyResponse {
///     fn response() -> (&'static str, RefOr<Response>) {
///         (
///             "MyResponse",
///             Response::new("My Response").into(),
///         )
///     }
/// }
/// ```
///
/// [derive]: derive.AsResponse.html
pub trait AsResponse {
    /// Returns a tuple of response component name (to be referenced) to a response.
    fn response() -> (&'static str, RefOr<crate::Response>);
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_partial_schema() {
        for (name, schema, value) in [
            ("i8", i8::schema(), json!({"type": "integer", "format": "int32"})),
            ("i16", i16::schema(), json!({"type": "integer", "format": "int32"})),
            ("i32", i32::schema(), json!({"type": "integer", "format": "int32"})),
            ("i64", i64::schema(), json!({"type": "integer", "format": "int64"})),
            ("i128", i128::schema(), json!({"type": "integer"})),
            ("isize", isize::schema(), json!({"type": "integer"})),
            (
                "u8",
                u8::schema(),
                json!({"type": "integer", "format": "int32", "minimum": 0.0}),
            ),
            (
                "u16",
                u16::schema(),
                json!({"type": "integer", "format": "int32", "minimum": 0.0}),
            ),
            (
                "u32",
                u32::schema(),
                json!({"type": "integer", "format": "int32", "minimum": 0.0}),
            ),
            (
                "u64",
                u64::schema(),
                json!({"type": "integer", "format": "int64", "minimum": 0.0}),
            ),
            ("u128", u128::schema(), json!({"type": "integer", "minimum": 0.0})),
            ("usize", usize::schema(), json!({"type": "integer", "minimum": 0.0 })),
            ("bool", bool::schema(), json!({"type": "boolean"})),
            ("str", str::schema(), json!({"type": "string"})),
            ("String", String::schema(), json!({"type": "string"})),
            ("char", char::schema(), json!({"type": "string"})),
            ("f32", f32::schema(), json!({"type": "number", "format": "float"})),
            ("f64", f64::schema(), json!({"type": "number", "format": "double"})),
        ] {
            println!("{name}: {json}", json = serde_json::to_string(&schema.1).unwrap());
            let schema = serde_json::to_value(schema.1).unwrap();
            assert_json_eq!(schema, value);
        }
    }
}
