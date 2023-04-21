#![cfg_attr(doc_cfg, feature(doc_cfg))]

mod openapi;
pub use openapi::*;
mod registries;
mod router;
pub mod swagger;

pub use registries::{OperationRegistry, SchemaRegistry};
pub use salvo_oapi_macros::*;
use std::collections::{BTreeMap, HashMap};

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
///     fn schema() -> (&'__s str, salvo_oapi::RefOr<salvo_oapi::schema::Schema>) {
///          (
///             "Pet",
///             salvo_oapi::Object::new()
///                 .property(
///                     "id",
///                     salvo_oapi::Object::new()
///                         .schema_type(salvo_oapi::SchemaType::Integer)
///                         .format(Some(salvo_oapi::SchemaFormat::KnownFormat(
///                             salvo_oapi::KnownFormat::Int64,
///                         ))),
///                 )
///                 .required("id")
///                 .property(
///                     "name",
///                     salvo_oapi::Object::new()
///                         .schema_type(salvo_oapi::SchemaType::String),
///                 )
///                 .required("name")
///                 .property(
///                     "age",
///                     salvo_oapi::Object::new()
///                         .schema_type(salvo_oapi::SchemaType::Integer)
///                         .format(Some(salvo_oapi::SchemaFormat::KnownFormat(
///                             salvo_oapi::KnownFormat::Int32,
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
    fn schema() -> (&'__s str, RefOr<schema::Schema>);

    /// Optional set of alias schemas for the [`ToSchema::schema`].
    ///
    /// Typically there is no need to manually implement this method but it is instead implemented
    /// by derive [`macro@ToSchema`] when `#[aliases(...)]` attribute is defined.
    fn aliases() -> Vec<(&'__s str, schema::Schema)> {
        Vec::new()
    }
}

impl<'__s, T: ToSchema<'__s>> From<T> for RefOr<schema::Schema> {
    fn from(_: T) -> Self {
        T::schema().1
    }
}

/// Represents _`nullable`_ type. This can be used anywhere where "nothing" needs to be evaluated.
/// This will serialize to _`null`_ in JSON and [`schema::empty`] is used to create the
/// [`schema::Schema`] for the type.
pub type TupleUnit = ();

impl<'__s> ToSchema<'__s> for TupleUnit {
    fn schema() -> (&'__s str, RefOr<schema::Schema>) {
        ("TupleUnit", schema::empty().into())
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
            fn schema() -> crate::RefOr<crate::schema::Schema> {
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
// Create `salvo-oapi` module so we can use `salvo-oapi-macros` directly
// from `salvo-oapi` crate. ONLY FOR INTERNAL USE!
#[doc(hidden)]
pub mod __private {
    pub use inventory;
}

pub trait Modifier<T> {
    fn modify(target: &mut T);
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
/// ```rust
/// # use salvo::oapi::PartialSchema;
/// # use salvo::oapi::schema::{SchemaType, KnownFormat, SchemaFormat, ObjectBuilder, Schema};
/// # use salvo::oapi::RefOr;
/// #
/// struct MyType;
///
/// impl PartialSchema for MyType {
///     fn schema() -> RefOr<Schema> {
///         // ... impl schema generation here
///         RefOr::T(Schema::Object(ObjectBuilder::new().build()))
///     }
/// }
/// ```
///
/// # Examples
///
/// _**Create number schema from u64.**_
/// ```rust
/// # use salvo::oapi::PartialSchema;
/// # use salvo::oapi::schema::{SchemaType, KnownFormat, SchemaFormat, ObjectBuilder, Schema};
/// # use salvo::oapi::RefOr;
/// #
/// let number: RefOr<Schema> = u64::schema().into();
///
/// // would be equal to manual implementation
/// let number2 = RefOr::T(
///     Schema::Object(
///         ObjectBuilder::new()
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
/// ```rust
/// # use salvo::oapi::PartialSchema;
/// # use salvo::oapi::schema::ObjectBuilder;
/// struct Pet {
///     id: i32,
///     name: String,
/// }
///
/// let pet_schema = ObjectBuilder::new()
///     .property("id", i32::schema())
///     .property("name", String::schema())
///     .required("id").required("name")
///     .build();
/// ```
///
/// [primitive]: https://doc.rust-lang.org/std/primitive/index.html
pub trait PartialSchema {
    /// Return ref or schema of implementing type that can then be used to
    /// construct combined schemas.
    fn schema() -> crate::RefOr<crate::schema::Schema>;
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
    fn schema() -> RefOr<schema::Schema> {
        schema!(#[inline] Vec<T>).into()
    }
}

impl<'__s, T: ToSchema<'__s>> PartialSchema for Option<Vec<T>> {
    fn schema() -> RefOr<schema::Schema> {
        schema!(#[inline] Option<Vec<T>>).into()
    }
}

impl<'__s, T: ToSchema<'__s>> PartialSchema for [T] {
    fn schema() -> RefOr<schema::Schema> {
        schema!(
            #[inline]
            [T]
        )
        .into()
    }
}

impl<'__s, T: ToSchema<'__s>> PartialSchema for &[T] {
    fn schema() -> RefOr<schema::Schema> {
        schema!(
            #[inline]
            &[T]
        )
        .into()
    }
}

impl<'__s, T: ToSchema<'__s>> PartialSchema for &mut [T] {
    fn schema() -> RefOr<schema::Schema> {
        schema!(
            #[inline]
            &mut [T]
        )
        .into()
    }
}

impl<'__s, T: ToSchema<'__s>> PartialSchema for Option<&[T]> {
    fn schema() -> RefOr<schema::Schema> {
        schema!(
            #[inline]
            Option<&[T]>
        )
        .into()
    }
}

impl<'__s, T: ToSchema<'__s>> PartialSchema for Option<&mut [T]> {
    fn schema() -> RefOr<schema::Schema> {
        schema!(
            #[inline]
            Option<&mut [T]>
        )
        .into()
    }
}

impl<'__s, T: ToSchema<'__s>> PartialSchema for Option<T> {
    fn schema() -> RefOr<schema::Schema> {
        schema!(#[inline] Option<T>).into()
    }
}

impl<'__s, K: PartialSchema, V: ToSchema<'__s>> PartialSchema for BTreeMap<K, V> {
    fn schema() -> RefOr<schema::Schema> {
        schema!(
            #[inline]
            BTreeMap<K, V>
        )
        .into()
    }
}

impl<'__s, K: PartialSchema, V: ToSchema<'__s>> PartialSchema for Option<BTreeMap<K, V>> {
    fn schema() -> RefOr<schema::Schema> {
        schema!(
            #[inline]
            Option<BTreeMap<K, V>>
        )
        .into()
    }
}

impl<'__s, K: PartialSchema, V: ToSchema<'__s>> PartialSchema for HashMap<K, V> {
    fn schema() -> RefOr<schema::Schema> {
        schema!(
            #[inline]
            HashMap<K, V>
        )
        .into()
    }
}

impl<'__s, K: PartialSchema, V: ToSchema<'__s>> PartialSchema for Option<HashMap<K, V>> {
    fn schema() -> RefOr<schema::Schema> {
        schema!(
            #[inline]
            Option<HashMap<K, V>>
        )
        .into()
    }
}

/// Trait used to convert implementing type to OpenAPI parameters.
///
/// This trait is [derivable][derive] for structs which are used to describe `path` or `query` parameters.
/// For more details of `#[derive(IntoParameters)]` refer to [derive documentation][derive].
///
/// # Examples
///
/// Derive [`IntoParameters`] implementation. This example will fail to compile because [`IntoParameters`] cannot
/// be used alone and it need to be used together with endpoint using the params as well. See
/// [derive documentation][derive] for more details.
/// ```
/// use salvo_oapi::{IntoParameters};
///
/// #[derive(IntoParameters)]
/// struct PetParams {
///     /// Id of pet
///     id: i64,
///     /// Name of pet
///     name: String,
/// }
/// ```
///
/// Roughly equal manual implementation of [`IntoParameters`] trait.
/// ```
/// # struct PetParams {
/// #    /// Id of pet
/// #    id: i64,
/// #    /// Name of pet
/// #    name: String,
/// # }
/// impl salvo_oapi::IntoParameters for PetParams {
///     fn into_parameters(
///         parameter_in_provider: impl Fn() -> Option<salvo_oapi::path::ParameterIn>
///     ) -> Vec<salvo_oapi::path::Parameter> {
///         vec![
///             salvo_oapi::path::Parameter::new()
///                 .name("id")
///                 .required(salvo_oapi::Required::True)
///                 .parameter_in(parameter_in_provider().unwrap_or_default())
///                 .description("Id of pet")
///                 .schema(Some(
///                     salvo_oapi::Object::new()
///                         .schema_type(salvo_oapi::SchemaType::Integer)
///                         .format(salvo_oapi::SchemaFormat::KnownFormat(salvo_oapi::KnownFormat::Int64)),
///                 )),
///             salvo_oapi::path::Parameter::new()
///                 .name("name")
///                 .required(salvo_oapi::Required::True)
///                 .parameter_in(parameter_in_provider().unwrap_or_default())
///                 .description("Name of pet")
///                 .schema(
///                     salvo_oapi::Object::new()
///                         .schema_type(salvo_oapi::SchemaType::String),
///                 ),
///         ]
///     }
/// }
/// ```
/// [derive]: derive.IntoParameters.html
pub trait IntoParameters {
    /// Provide [`Vec`] of [`Parameter`]s to caller. The result is used in `salvo-oapi-macros` library to
    /// provide OpenAPI parameter information for the endpoint using the parameters.
    fn into_parameters(parameter_in_provider: impl Fn() -> Option<ParameterIn>) -> Vec<Parameter>;
}

/// This trait is implemented to document a type (like an enum) which can represent multiple
/// responses, to be used in operation.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
/// use salvo_oapi::{Response, Response, Responses, RefOr, IntoResponses };
///
/// enum MyResponse {
///     Ok,
///     NotFound,
/// }
///
/// impl IntoResponses for MyResponse {
///     fn responses() -> BTreeMap<String, RefOr<Response>> {
///         Responses::new()
///             .response("200", Response::new("Ok"))
///             .response("404", Response::new("Not Found"))
///             .into()
///     }
/// }
/// ```
pub trait IntoResponses {
    /// Returns an ordered map of response codes to responses.
    fn responses() -> BTreeMap<String, openapi::RefOr<openapi::Response>>;
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
///             Response::new("My Response").into(),
///         )
///     }
/// }
/// ```
///
/// [derive]: derive.ToResponse.html
pub trait ToResponse<'__r> {
    /// Returns a tuple of response component name (to be referenced) to a response.
    fn response() -> (&'__r str, RefOr<crate::Response>);
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
            println!("{name}: {json}", json = serde_json::to_string(&schema).unwrap());
            let schema = serde_json::to_value(schema).unwrap();
            assert_json_eq!(schema, value);
        }
    }
}
