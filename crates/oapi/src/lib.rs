#![cfg_attr(doc_cfg, feature(doc_cfg))]

pub mod openapi;
pub mod swagger;

use std::collections::{BTreeMap, HashMap};
pub use salvo_oapi_macros::*;

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
/// # use salvo_oapi::ToSchema;
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
/// # struct Pet {
/// #     id: u64,
/// #     name: String,
/// #     age: Option<i32>,
/// # }
/// #
/// impl<'__s> salvo_oapi::ToSchema<'__s> for Pet {
///     fn schema() -> (&'__s str, salvo_oapi::openapi::RefOr<salvo_oapi::openapi::schema::Schema>) {
///          (
///             "Pet",
///             salvo_oapi::openapi::Object::new()
///                 .property(
///                     "id",
///                     salvo_oapi::openapi::Object::new()
///                         .schema_type(salvo_oapi::openapi::SchemaType::Integer)
///                         .format(Some(salvo_oapi::openapi::SchemaFormat::KnownFormat(
///                             salvo_oapi::openapi::KnownFormat::Int64,
///                         ))),
///                 )
///                 .required("id")
///                 .property(
///                     "name",
///                     salvo_oapi::openapi::Object::new()
///                         .schema_type(salvo_oapi::openapi::SchemaType::String),
///                 )
///                 .required("name")
///                 .property(
///                     "age",
///                     salvo_oapi::openapi::Object::new()
///                         .schema_type(salvo_oapi::openapi::SchemaType::Integer)
///                         .format(Some(salvo_oapi::openapi::SchemaFormat::KnownFormat(
///                             salvo_oapi::openapi::KnownFormat::Int32,
///                         ))),
///                 )
///                 .example(Some(serde_json::json!({
///                   "name":"bob the cat","id":1
///                 })))
///                 .into(),
///         ) }
/// }
/// ```
pub trait ToSchema<'__s> {
    /// Return a tuple of name and schema or reference to a schema that can be referenced by the
    /// name or inlined directly to responses, request bodies or parameters.
    fn schema() -> (&'__s str, openapi::RefOr<openapi::schema::Schema>);

    /// Optional set of alias schemas for the [`ToSchema::schema`].
    ///
    /// Typically there is no need to manually implement this method but it is instead implemented
    /// by derive [`macro@ToSchema`] when `#[aliases(...)]` attribute is defined.
    fn aliases() -> Vec<(&'__s str, openapi::schema::Schema)> {
        Vec::new()
    }
}

impl<'__s, T: ToSchema<'__s>> From<T> for openapi::RefOr<openapi::schema::Schema> {
    fn from(_: T) -> Self {
        T::schema().1
    }
}

/// Represents _`nullable`_ type. This can be used anywhere where "nothing" needs to be evaluated.
/// This will serialize to _`null`_ in JSON and [`openapi::schema::empty`] is used to create the
/// [`openapi::schema::Schema`] for the type.
pub type TupleUnit = ();

impl<'__s> ToSchema<'__s> for TupleUnit {
    fn schema() -> (&'__s str, openapi::RefOr<openapi::schema::Schema>) {
        ("TupleUnit", openapi::schema::empty().into())
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
        impl PartialSchema for $($tt)* {
            fn schema() -> openapi::RefOr<openapi::schema::Schema> {
                schema!( $($tt)* ).into()
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
mod oapi {
    pub use super::*;
}

/// Trait used to implement only _`Schema`_ part of the OpenAPI doc.
///
/// This trait is by default implemented for Rust [`primitive`][primitive] types and some well known types like
/// [`Vec`], [`Option`], [`HashMap`] and [`BTreeMap`]. The default implementation adds `schema()`
/// method to the implementing type allowing simple conversion of the type to the OpenAPI Schema
/// object. Moreover this allows handy way of constructing schema objects manually if ever so
/// wished.
///
/// The trait can be implemented manually easily on any type. This trait comes especially handy
/// with [`macro@schema`] macro that can be used to generate schema for arbitrary types.
/// ```
/// # use salvo_oapi::PartialSchema;
/// # use salvo_oapi::openapi::schema::{SchemaType, KnownFormat, SchemaFormat, Object, Schema};
/// # use salvo_oapi::openapi::RefOr;
/// #
/// struct MyType;
///
/// impl PartialSchema for MyType {
///     fn schema() -> RefOr<Schema> {
///         // ... impl schema generation here
///         RefOr::T(Schema::Object(Object::new().build()))
///     }
/// }
/// ```
///
/// # Examples
///
/// _**Create number schema from u64.**_
/// ```
/// # use salvo_oapi::PartialSchema;
/// # use salvo_oapi::openapi::schema::{SchemaType, KnownFormat, SchemaFormat, Object, Schema};
/// # use salvo_oapi::openapi::RefOr;
/// #
/// let number: RefOr<Schema> = u64::schema().into();
///
/// // would be equal to manual implementation
/// let number2 = RefOr::T(
///     Schema::Object(
///         Object::new()
///             .schema_type(SchemaType::Integer)
///             .format(Some(SchemaFormat::KnownFormat(KnownFormat::Int64)))
///             .minimum(Some(0.0))
///             .build()
///         )
///     );
/// # assert_json_diff::assert_json_eq!(serde_json::to_value(&number).unwrap(), serde_json::to_value(&number2).unwrap());
/// ```
///
/// _**Construct a Pet object schema manually.**_
/// ```
/// # use salvo_oapi::PartialSchema;
/// # use salvo_oapi::openapi::schema::Object;
/// struct Pet {
///     id: i32,
///     name: String,
/// }
///
/// let pet_schema = Object::new()
///     .property("id", i32::schema())
///     .property("name", String::schema())
///     .required("id").required("name")
///     .build();
/// ```
///
/// [primitive]: https://doc.rust-lang.org/std/primitive/index.html
pub trait PartialSchema {
    /// Return ref or schema of implementing type that can then be used to construct combined schemas.
    fn schema() -> openapi::RefOr<openapi::schema::Schema>;
}

#[rustfmt::skip]
impl_partial_schema_primitive!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, bool, f32, f64, String, str, char,
    Option<i8>, Option<i16>, Option<i32>, Option<i64>, Option<i128>, Option<isize>, Option<u8>, Option<u16>, 
    Option<u32>, Option<u64>, Option<u128>, Option<usize>, Option<bool>, Option<f32>, Option<f64>,
    Option<String>, Option<&str>, Option<char>
);

impl_partial_schema!(&str);

impl<'__s, T: ToSchema<'__s>> PartialSchema for Vec<T> {
    fn schema() -> openapi::RefOr<openapi::schema::Schema> {
        schema!(#[inline] Vec<T>).into()
    }
}

impl<'__s, T: ToSchema<'__s>> PartialSchema for Option<Vec<T>> {
    fn schema() -> openapi::RefOr<openapi::schema::Schema> {
        schema!(#[inline] Option<Vec<T>>).into()
    }
}

impl<'__s, T: ToSchema<'__s>> PartialSchema for [T] {
    fn schema() -> openapi::RefOr<openapi::schema::Schema> {
        schema!(
            #[inline]
            [T]
        )
        .into()
    }
}

impl<'__s, T: ToSchema<'__s>> PartialSchema for &[T] {
    fn schema() -> openapi::RefOr<openapi::schema::Schema> {
        schema!(
            #[inline]
            &[T]
        )
        .into()
    }
}

impl<'__s, T: ToSchema<'__s>> PartialSchema for &mut [T] {
    fn schema() -> openapi::RefOr<openapi::schema::Schema> {
        schema!(
            #[inline]
            &mut [T]
        )
        .into()
    }
}

impl<'__s, T: ToSchema<'__s>> PartialSchema for Option<&[T]> {
    fn schema() -> openapi::RefOr<openapi::schema::Schema> {
        schema!(
            #[inline]
            Option<&[T]>
        )
        .into()
    }
}

impl<'__s, T: ToSchema<'__s>> PartialSchema for Option<&mut [T]> {
    fn schema() -> openapi::RefOr<openapi::schema::Schema> {
        schema!(
            #[inline]
            Option<&mut [T]>
        )
        .into()
    }
}

impl<'__s, T: ToSchema<'__s>> PartialSchema for Option<T> {
    fn schema() -> openapi::RefOr<openapi::schema::Schema> {
        schema!(#[inline] Option<T>).into()
    }
}

impl<'__s, K: PartialSchema, V: ToSchema<'__s>> PartialSchema for BTreeMap<K, V> {
    fn schema() -> openapi::RefOr<openapi::schema::Schema> {
        schema!(
            #[inline]
            BTreeMap<K, V>
        )
        .into()
    }
}

impl<'__s, K: PartialSchema, V: ToSchema<'__s>> PartialSchema for Option<BTreeMap<K, V>> {
    fn schema() -> openapi::RefOr<openapi::schema::Schema> {
        schema!(
            #[inline]
            Option<BTreeMap<K, V>>
        )
        .into()
    }
}

impl<'__s, K: PartialSchema, V: ToSchema<'__s>> PartialSchema for HashMap<K, V> {
    fn schema() -> openapi::RefOr<openapi::schema::Schema> {
        schema!(
            #[inline]
            HashMap<K, V>
        )
        .into()
    }
}

impl<'__s, K: PartialSchema, V: ToSchema<'__s>> PartialSchema for Option<HashMap<K, V>> {
    fn schema() -> openapi::RefOr<openapi::schema::Schema> {
        schema!(
            #[inline]
            Option<HashMap<K, V>>
        )
        .into()
    }
}

/// Trait for implementing OpenAPI PathItem object with path.
///
/// This trait is implemented via [`#[salvo_oapi::path(...)]`][derive] attribute macro and there
/// is no need to implement this trait manually.
///
/// # Examples
///
/// Use `#[salvo_oapi::path(..)]` to implement Path trait
/// ```
/// # struct Pet {
/// #   id: u64,
/// #   name: String,
/// # }
/// #
/// #
/// /// Get pet by id
/// ///
/// /// Get pet from database by pet database id
/// #[salvo_oapi::path(
///     get,
///     path = "/pets/{id}",
///     responses(
///         (status = 200, description = "Pet found successfully", body = Pet),
///         (status = 404, description = "Pet was not found")
///     ),
///     params(
///         ("id" = u64, Path, description = "Pet database id to get Pet for"),
///     )
/// )]
/// async fn get_pet_by_id(pet_id: u64) -> Pet {
///     Pet {
///         id: pet_id,
///         name: "lightning".to_string(),
///     }
/// }
/// ```
///
/// Example of what would manual implementation roughly look like of above `#[salvo_oapi::path(...)]` macro.
/// ```
/// salvo_oapi::openapi::Paths::new().path(
///         "/pets/{id}",
///         salvo_oapi::openapi::PathItem::new(
///             salvo_oapi::openapi::PathItemType::Get,
///             salvo_oapi::openapi::path::OperationBuilder::new()
///                 .responses(
///                     salvo_oapi::openapi::ResponsesBuilder::new()
///                         .response(
///                             "200",
///                             salvo_oapi::openapi::Response::new()
///                                 .description("Pet found successfully")
///                                 .content("application/json",
///                                     salvo_oapi::openapi::Content::new(
///                                         salvo_oapi::openapi::Ref::from_schema_name("Pet"),
///                                     ),
///                             ),
///                         )
///                         .response("404", salvo_oapi::openapi::Response::new("Pet was not found")),
///                 )
///                 .operation_id(Some("get_pet_by_id"))
///                 .deprecated(Some(salvo_oapi::openapi::Deprecated::False))
///                 .summary(Some("Get pet by id"))
///                 .description(Some("Get pet by id\n\nGet pet from database by pet database id\n"))
///                 .parameter(
///                     salvo_oapi::openapi::path::ParameterBuilder::new()
///                         .name("id")
///                         .parameter_in(salvo_oapi::openapi::path::ParameterIn::Path)
///                         .required(salvo_oapi::openapi::Required::True)
///                         .deprecated(Some(salvo_oapi::openapi::Deprecated::False))
///                         .description(Some("Pet database id to get Pet for"))
///                         .schema(
///                             Some(salvo_oapi::openapi::Object::new()
///                                 .schema_type(salvo_oapi::openapi::SchemaType::Integer)
///                                 .format(Some(salvo_oapi::openapi::SchemaFormat::KnownFormat(salvo_oapi::openapi::KnownFormat::Int64)))),
///                         ),
///                 )
///                 .tag("pet_api"),
///         ),
///     );
/// ```
///
/// [derive]: attr.path.html
pub trait Path {
    fn path() -> &'static str;

    fn path_item(default_tag: Option<&str>) -> openapi::path::PathItem;
}

/// Trait that allows OpenApi modification at runtime.
///
/// Implement this trait if you wish to modify the OpenApi at runtime before it is being consumed
/// *(Before `salvo_oapi::OpenApi::openapi()` function returns)*.
/// This is trait can be used to add or change already generated OpenApi spec to alter the generated
/// specification by user defined condition. For example you can add definitions that should be loaded
/// from some configuration at runtime what may not be available during compile time.
///
/// See more about [`OpenApi`][derive] derive at [derive documentation][derive].
///
/// [derive]: derive.OpenApi.html
/// [security_schema]: openapi/security/enum.SecuritySchema.html
///
/// # Examples
///
/// Add custom JWT [`SecuritySchema`][security_schema] to [`OpenApi`][`openapi::OpenApi`].
/// ```
/// # use salvo_oapi::{OpenApi, Modify};
/// # use salvo_oapi::openapi::security::{SecurityScheme, HttpBuilder, HttpAuthScheme};
/// #[derive(OpenApi)]
/// #[openapi(modifiers(&SecurityAddon))]
/// struct ApiDoc;
///
/// struct SecurityAddon;
///
/// impl Modify for SecurityAddon {
///     fn modify(&self, openapi: &mut salvo_oapi::openapi::OpenApi) {
///          openapi.components = Some(
///              salvo_oapi::openapi::Components::new()
///                  .security_scheme(
///                      "api_jwt_token",
///                      SecurityScheme::Http(
///                          HttpBuilder::new()
///                              .scheme(HttpAuthScheme::Bearer)
///                              .bearer_format("JWT")
///                              .build(),
///                      ),
///                  )
///                  .build(),
///          )
///      }
/// }
/// ```
///
/// Add [OpenAPI Server Object][server] to alter the target server url. This can be used to give context
/// path for api operations.
/// ```
/// # use salvo_oapi::{OpenApi, Modify};
/// # use salvo_oapi::openapi::Server;
/// #[derive(OpenApi)]
/// #[openapi(modifiers(&ServerAddon))]
/// struct ApiDoc;
///
/// struct ServerAddon;
///
/// impl Modify for ServerAddon {
///     fn modify(&self, openapi: &mut salvo_oapi::openapi::OpenApi) {
///         openapi.servers = Some(vec![Server::new("/api")])
///     }
/// }
/// ```
///
/// [server]: https://spec.openapis.org/oas/latest.html#server-object
pub trait Modify {
    fn modify(&self, openapi: &mut openapi::OpenApi);
}

/// Trait used to convert implementing type to OpenAPI parameters.
///
/// This trait is [derivable][derive] for structs which are used to describe `path` or `query` parameters.
/// For more details of `#[derive(IntoParams)]` refer to [derive documentation][derive].
///
/// # Examples
///
/// Derive [`IntoParams`] implementation. This example will fail to compile because [`IntoParams`] cannot
/// be used alone and it need to be used together with endpoint using the params as well. See
/// [derive documentation][derive] for more details.
/// ```
/// use salvo_oapi::{IntoParams};
///
/// #[derive(IntoParams)]
/// struct PetParams {
///     /// Id of pet
///     id: i64,
///     /// Name of pet
///     name: String,
/// }
/// ```
///
/// Roughly equal manual implementation of [`IntoParams`] trait.
/// ```
/// # struct PetParams {
/// #    /// Id of pet
/// #    id: i64,
/// #    /// Name of pet
/// #    name: String,
/// # }
/// impl salvo_oapi::IntoParams for PetParams {
///     fn into_params(
///         parameter_in_provider: impl Fn() -> Option<salvo_oapi::openapi::path::ParameterIn>
///     ) -> Vec<salvo_oapi::openapi::path::Parameter> {
///         vec![
///             salvo_oapi::openapi::path::ParameterBuilder::new()
///                 .name("id")
///                 .required(salvo_oapi::openapi::Required::True)
///                 .parameter_in(parameter_in_provider().unwrap_or_default())
///                 .description(Some("Id of pet"))
///                 .schema(Some(
///                     salvo_oapi::openapi::Object::new()
///                         .schema_type(salvo_oapi::openapi::SchemaType::Integer)
///                         .format(Some(salvo_oapi::openapi::SchemaFormat::KnownFormat(salvo_oapi::openapi::KnownFormat::Int64))),
///                 ))
///                 .build(),
///             salvo_oapi::openapi::path::ParameterBuilder::new()
///                 .name("name")
///                 .required(salvo_oapi::openapi::Required::True)
///                 .parameter_in(parameter_in_provider().unwrap_or_default())
///                 .description(Some("Name of pet"))
///                 .schema(Some(
///                     salvo_oapi::openapi::Object::new()
///                         .schema_type(salvo_oapi::openapi::SchemaType::String),
///                 ))
///                 .build(),
///         ]
///     }
/// }
/// ```
/// [derive]: derive.IntoParams.html
pub trait IntoParams {
    /// Provide [`Vec`] of [`openapi::path::Parameter`]s to caller. The result is used in `salvo_oapi-macros` library to
    /// provide OpenAPI parameter information for the endpoint using the parameters.
    fn into_params(
        parameter_in_provider: impl Fn() -> Option<openapi::path::ParameterIn>,
    ) -> Vec<openapi::path::Parameter>;
}

/// This trait is implemented to document a type (like an enum) which can represent multiple
/// responses, to be used in operation.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use salvo_oapi::{
///     openapi::{Response, Response, ResponsesBuilder, RefOr},
///     IntoResponses,
/// };
///
/// enum MyResponse {
///     Ok,
///     NotFound,
/// }
///
/// impl IntoResponses for MyResponse {
///     fn responses() -> BTreeMap<String, RefOr<Response>> {
///         ResponsesBuilder::new()
///             .response("200", Response::new().description("Ok"))
///             .response("404", Response::new().description("Not Found"))
///             .build()
///             .into()
///     }
/// }
/// ```
pub trait IntoResponses {
    /// Returns an ordered map of response codes to responses.
    fn responses() -> BTreeMap<String, openapi::RefOr<openapi::response::Response>>;
}

/// This trait is implemented to document a type which represents a single response which can be
/// referenced or reused as a component in multiple operations.
///
/// _`ToResponse`_ trait can also be derived with [`#[derive(ToResponse)]`][derive].
///
/// # Examples
///
/// ```
/// use salvo_oapi::{
///     openapi::{RefOr, Response, Response},
///     ToResponse,
/// };
///
/// struct MyResponse;
///
/// impl<'__r> ToResponse<'__r> for MyResponse {
///     fn response() -> (&'__r str, RefOr<Response>) {
///         (
///             "MyResponse",
///             Response::new().description("My Response").build().into(),
///         )
///     }
/// }
/// ```
///
/// [derive]: derive.ToResponse.html
pub trait ToResponse<'__r> {
    /// Returns a tuple of response component name (to be referenced) to a response.
    fn response() -> (&'__r str, openapi::RefOr<openapi::response::Response>);
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_partial_schema() {
        for (name, schema, value) in [
            (
                "i8",
                i8::schema(),
                json!({"type": "integer", "format": "int32"}),
            ),
            (
                "i16",
                i16::schema(),
                json!({"type": "integer", "format": "int32"}),
            ),
            (
                "i32",
                i32::schema(),
                json!({"type": "integer", "format": "int32"}),
            ),
            (
                "i64",
                i64::schema(),
                json!({"type": "integer", "format": "int64"}),
            ),
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
            (
                "u128",
                u128::schema(),
                json!({"type": "integer", "minimum": 0.0}),
            ),
            (
                "usize",
                usize::schema(),
                json!({"type": "integer", "minimum": 0.0 }),
            ),
            ("bool", bool::schema(), json!({"type": "boolean"})),
            ("str", str::schema(), json!({"type": "string"})),
            ("String", String::schema(), json!({"type": "string"})),
            ("char", char::schema(), json!({"type": "string"})),
            (
                "f32",
                f32::schema(),
                json!({"type": "number", "format": "float"}),
            ),
            (
                "f64",
                f64::schema(),
                json!({"type": "number", "format": "double"}),
            ),
        ] {
            println!(
                "{name}: {json}",
                json = serde_json::to_string(&schema).unwrap()
            );
            let schema = serde_json::to_value(schema).unwrap();
            assert_json_eq!(schema, value);
        }
    }
}
