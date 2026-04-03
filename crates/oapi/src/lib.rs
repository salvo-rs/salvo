#![doc = include_str!("../docs/lib.md")]
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, allow(clippy::unwrap_used))]

#[macro_use]
mod cfg;

mod openapi;
pub use openapi::*;

#[doc = include_str!("../docs/endpoint.md")]
pub mod endpoint;
pub use endpoint::{Endpoint, EndpointArgRegister, EndpointOutRegister, EndpointRegistry};
pub mod extract;
mod routing;
pub use routing::RouterExt;
/// Module for name schemas.
pub mod naming;

cfg_feature! {
    #![feature ="swagger-ui"]
    pub mod swagger_ui;
}
cfg_feature! {
    #![feature ="scalar"]
    pub mod scalar;
}
cfg_feature! {
    #![feature ="rapidoc"]
    pub mod rapidoc;
}
cfg_feature! {
    #![feature ="redoc"]
    pub mod redoc;
}

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, LinkedList};
use std::marker::PhantomData;

use salvo_core::extract::Extractible;
use salvo_core::http::StatusError;
use salvo_core::writing;
#[doc = include_str!("../docs/derive_to_parameters.md")]
pub use salvo_oapi_macros::ToParameters;
#[doc = include_str!("../docs/derive_to_response.md")]
pub use salvo_oapi_macros::ToResponse;
#[doc = include_str!("../docs/derive_to_responses.md")]
pub use salvo_oapi_macros::ToResponses;
#[doc = include_str!("../docs/derive_to_schema.md")]
pub use salvo_oapi_macros::ToSchema;
#[doc = include_str!("../docs/endpoint.md")]
pub use salvo_oapi_macros::endpoint;
pub(crate) use salvo_oapi_macros::schema;

use crate::oapi::openapi::schema::OneOf;

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
/// #[salvo(schema(example = json!({"name": "bob the cat", "id": 1})))]
/// struct Pet {
///     id: u64,
///     name: String,
///     age: Option<i32>,
/// }
/// ```
///
/// Following manual implementation is equal to above derive one.
/// ```
/// use salvo_oapi::{Components, ToSchema, RefOr, Schema, SchemaFormat, BasicType, SchemaType, KnownFormat, Object};
/// # struct Pet {
/// #     id: u64,
/// #     name: String,
/// #     age: Option<i32>,
/// # }
/// #
/// impl ToSchema for Pet {
///     fn to_schema(components: &mut Components) -> RefOr<Schema> {
///         Object::new()
///             .property(
///                 "id",
///                 Object::new()
///                     .schema_type(BasicType::Integer)
///                     .format(SchemaFormat::KnownFormat(
///                         KnownFormat::Int64,
///                     )),
///             )
///             .required("id")
///             .property(
///                 "name",
///                 Object::new()
///                     .schema_type(BasicType::String),
///             )
///             .required("name")
///             .property(
///                 "age",
///                 Object::new()
///                     .schema_type(BasicType::Integer)
///                     .format(SchemaFormat::KnownFormat(
///                         KnownFormat::Int32,
///                     )),
///             )
///             .example(serde_json::json!({
///               "name":"bob the cat","id":1
///             }))
///             .into()
///     }
/// }
/// ```
pub trait ToSchema {
    /// Returns a tuple of name and schema or reference to a schema that can be referenced by the
    /// name or inlined directly to responses, request bodies or parameters.
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema>;
}

/// Trait for composing schemas with generic type parameters.
///
/// `ComposeSchema` enables generic types to compose their schemas from externally-provided
/// generic parameter schemas. This separates schema structure generation (compose) from
/// naming and registration (ToSchema).
///
/// For non-generic types, the `generics` parameter is ignored and the schema is generated
/// directly. For generic types, each element in `generics` corresponds to a type parameter's
/// schema, in declaration order.
///
/// # Examples
///
/// Manual implementation for a generic wrapper type:
/// ```
/// use salvo_oapi::{BasicType, Components, ComposeSchema, Object, RefOr, Schema};
///
/// struct Page<T> {
///     items: Vec<T>,
///     total: u64,
/// }
///
/// impl<T: ComposeSchema> ComposeSchema for Page<T> {
///     fn compose(components: &mut Components, generics: Vec<RefOr<Schema>>) -> RefOr<Schema> {
///         let t_schema = generics
///             .first()
///             .cloned()
///             .unwrap_or_else(|| T::compose(components, vec![]));
///         Object::new()
///             .property("items", salvo_oapi::schema::Array::new().items(t_schema))
///             .required("items")
///             .property("total", Object::new().schema_type(BasicType::Integer))
///             .required("total")
///             .into()
///     }
/// }
/// ```
pub trait ComposeSchema {
    /// Compose a schema using the provided generic parameter schemas.
    ///
    /// The `components` parameter allows registering nested schemas.
    /// The `generics` vector contains pre-resolved schemas for each type parameter,
    /// in the order they appear in the type definition.
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema>;
}

/// Tracks schema references for generic type resolution.
///
/// `SchemaReference` represents a schema and its generic parameter references,
/// enabling recursive schema composition for generic types.
#[derive(Debug, Clone, Default)]
pub struct SchemaReference {
    /// The schema name.
    pub name: std::borrow::Cow<'static, str>,
    /// Whether this schema should be inlined rather than referenced.
    pub inline: bool,
    /// Child references for generic type parameters.
    pub references: Vec<Self>,
}

impl SchemaReference {
    /// Create a new `SchemaReference` with the given name.
    pub fn new(name: impl Into<std::borrow::Cow<'static, str>>) -> Self {
        Self {
            name: name.into(),
            inline: false,
            references: Vec::new(),
        }
    }

    /// Set whether this schema should be inlined.
    #[must_use]
    pub fn inline(mut self, inline: bool) -> Self {
        self.inline = inline;
        self
    }

    /// Add a child reference for a generic type parameter.
    #[must_use]
    pub fn reference(mut self, reference: Self) -> Self {
        self.references.push(reference);
        self
    }

    /// Get the composed name including generic parameters.
    ///
    /// For example, `Page` with child `User` produces `Page<User>`.
    #[must_use]
    pub fn compose_name(&self) -> String {
        if self.references.is_empty() {
            self.name.to_string()
        } else {
            let generic_names: Vec<String> =
                self.references.iter().map(|r| r.compose_name()).collect();
            format!("{}<{}>", self.name, generic_names.join(", "))
        }
    }

    /// Get the schemas for the direct generic type parameters.
    #[must_use]
    pub fn compose_generics(&self) -> &[Self] {
        &self.references
    }

    /// Collect all child references recursively (depth-first).
    #[must_use]
    pub fn compose_child_references(&self) -> Vec<&Self> {
        let mut result = Vec::new();
        for reference in &self.references {
            result.push(reference);
            result.extend(reference.compose_child_references());
        }
        result
    }
}

/// Represents _`nullable`_ type.
///
/// This can be used anywhere where "nothing" needs to be evaluated.
/// This will serialize to _`null`_ in JSON and [`schema::empty`] is used to create the
/// [`schema::Schema`] for the type.
pub type TupleUnit = ();

impl ToSchema for TupleUnit {
    fn to_schema(_components: &mut Components) -> RefOr<schema::Schema> {
        schema::empty().into()
    }
}
impl ComposeSchema for TupleUnit {
    fn compose(
        _components: &mut Components,
        _generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        schema::empty().into()
    }
}

macro_rules! impl_to_schema {
    ($ty:path) => {
        impl_to_schema!( @impl_schema $ty );
    };
    (&$ty:path) => {
        impl_to_schema!( @impl_schema &$ty );
    };
    (@impl_schema $($tt:tt)*) => {
        impl ToSchema for $($tt)* {
            fn to_schema(_components: &mut Components) -> crate::RefOr<crate::schema::Schema> {
                 schema!( $($tt)* ).into()
            }
        }
        impl ComposeSchema for $($tt)* {
            fn compose(_components: &mut Components, _generics: Vec<crate::RefOr<crate::schema::Schema>>) -> crate::RefOr<crate::schema::Schema> {
                 schema!( $($tt)* ).into()
            }
        }
    };
}

macro_rules! impl_to_schema_primitive {
    ($($tt:path),*) => {
        $( impl_to_schema!( $tt ); )*
    };
}

// Create `salvo-oapi` module so we can use `salvo-oapi-macros` directly
// from `salvo-oapi` crate. ONLY FOR INTERNAL USE!
#[doc(hidden)]
pub mod oapi {
    pub use super::*;
}

#[doc(hidden)]
pub mod __private {
    pub use inventory;
    pub use serde_json;
}

#[rustfmt::skip]
impl_to_schema_primitive!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, bool, f32, f64, String, str, char
);
impl_to_schema!(&str);

impl_to_schema!(std::net::Ipv4Addr);
impl_to_schema!(std::net::Ipv6Addr);

impl ToSchema for std::net::IpAddr {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        crate::RefOr::Type(Schema::OneOf(
            OneOf::default()
                .item(std::net::Ipv4Addr::to_schema(components))
                .item(std::net::Ipv6Addr::to_schema(components)),
        ))
    }
}
impl ComposeSchema for std::net::IpAddr {
    fn compose(
        components: &mut Components,
        _generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        Self::to_schema(components)
    }
}

#[cfg(feature = "chrono")]
impl_to_schema_primitive!(chrono::NaiveDate, chrono::Duration, chrono::NaiveDateTime);
#[cfg(feature = "chrono")]
impl<T: chrono::TimeZone> ToSchema for chrono::DateTime<T> {
    fn to_schema(_components: &mut Components) -> RefOr<schema::Schema> {
        schema!(#[inline] DateTime<T>).into()
    }
}
#[cfg(feature = "chrono")]
impl<T: chrono::TimeZone> ComposeSchema for chrono::DateTime<T> {
    fn compose(
        _components: &mut Components,
        _generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        schema!(#[inline] DateTime<T>).into()
    }
}
#[cfg(feature = "compact_str")]
impl_to_schema_primitive!(compact_str::CompactString);
#[cfg(any(feature = "decimal", feature = "decimal-float"))]
impl_to_schema!(rust_decimal::Decimal);
#[cfg(feature = "url")]
impl_to_schema!(url::Url);
#[cfg(feature = "uuid")]
impl_to_schema!(uuid::Uuid);
#[cfg(feature = "ulid")]
impl_to_schema!(ulid::Ulid);
#[cfg(feature = "time")]
impl_to_schema_primitive!(
    time::Date,
    time::PrimitiveDateTime,
    time::OffsetDateTime,
    time::Duration
);
#[cfg(feature = "smallvec")]
impl<T: ToSchema + smallvec::Array> ToSchema for smallvec::SmallVec<T> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        schema!(#[inline] smallvec::SmallVec<T>).into()
    }
}
#[cfg(feature = "smallvec")]
impl<T: ComposeSchema + smallvec::Array> ComposeSchema for smallvec::SmallVec<T> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        let t_schema = generics
            .first()
            .cloned()
            .unwrap_or_else(|| T::compose(components, vec![]));
        schema::Array::new().items(t_schema).into()
    }
}
#[cfg(feature = "indexmap")]
impl<K: ToSchema, V: ToSchema> ToSchema for indexmap::IndexMap<K, V> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        schema!(#[inline] indexmap::IndexMap<K, V>).into()
    }
}
#[cfg(feature = "indexmap")]
impl<K: ComposeSchema, V: ComposeSchema> ComposeSchema for indexmap::IndexMap<K, V> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        let v_schema = generics
            .get(1)
            .cloned()
            .unwrap_or_else(|| V::compose(components, vec![]));
        schema::Object::new().additional_properties(v_schema).into()
    }
}

impl<T: ToSchema> ToSchema for Vec<T> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        schema!(#[inline] Vec<T>).into()
    }
}
impl<T: ComposeSchema> ComposeSchema for Vec<T> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        let t_schema = generics
            .first()
            .cloned()
            .unwrap_or_else(|| T::compose(components, vec![]));
        schema::Array::new().items(t_schema).into()
    }
}

impl<T: ToSchema> ToSchema for LinkedList<T> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        schema!(#[inline] LinkedList<T>).into()
    }
}
impl<T: ComposeSchema> ComposeSchema for LinkedList<T> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        let t_schema = generics
            .first()
            .cloned()
            .unwrap_or_else(|| T::compose(components, vec![]));
        schema::Array::new().items(t_schema).into()
    }
}

impl<T: ToSchema> ToSchema for HashSet<T> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        schema::Array::new()
            .items(T::to_schema(components))
            .unique_items(true)
            .into()
    }
}
impl<T: ComposeSchema> ComposeSchema for HashSet<T> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        let t_schema = generics
            .first()
            .cloned()
            .unwrap_or_else(|| T::compose(components, vec![]));
        schema::Array::new()
            .items(t_schema)
            .unique_items(true)
            .into()
    }
}

impl<T: ToSchema> ToSchema for BTreeSet<T> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        schema::Array::new()
            .items(T::to_schema(components))
            .unique_items(true)
            .into()
    }
}
impl<T: ComposeSchema> ComposeSchema for BTreeSet<T> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        let t_schema = generics
            .first()
            .cloned()
            .unwrap_or_else(|| T::compose(components, vec![]));
        schema::Array::new()
            .items(t_schema)
            .unique_items(true)
            .into()
    }
}

#[cfg(feature = "indexmap")]
impl<T: ToSchema> ToSchema for indexmap::IndexSet<T> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        schema::Array::new()
            .items(T::to_schema(components))
            .unique_items(true)
            .into()
    }
}
#[cfg(feature = "indexmap")]
impl<T: ComposeSchema> ComposeSchema for indexmap::IndexSet<T> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        let t_schema = generics
            .first()
            .cloned()
            .unwrap_or_else(|| T::compose(components, vec![]));
        schema::Array::new()
            .items(t_schema)
            .unique_items(true)
            .into()
    }
}

impl<T: ToSchema> ToSchema for Box<T> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        T::to_schema(components)
    }
}
impl<T: ComposeSchema> ComposeSchema for Box<T> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        T::compose(components, generics)
    }
}

impl<T: ToSchema + ToOwned> ToSchema for std::borrow::Cow<'_, T> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        T::to_schema(components)
    }
}
impl<T: ComposeSchema + ToOwned> ComposeSchema for std::borrow::Cow<'_, T> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        T::compose(components, generics)
    }
}

impl<T: ToSchema> ToSchema for std::cell::RefCell<T> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        T::to_schema(components)
    }
}
impl<T: ComposeSchema> ComposeSchema for std::cell::RefCell<T> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        T::compose(components, generics)
    }
}

impl<T: ToSchema> ToSchema for std::rc::Rc<T> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        T::to_schema(components)
    }
}
impl<T: ComposeSchema> ComposeSchema for std::rc::Rc<T> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        T::compose(components, generics)
    }
}

impl<T: ToSchema> ToSchema for std::sync::Arc<T> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        T::to_schema(components)
    }
}
impl<T: ComposeSchema> ComposeSchema for std::sync::Arc<T> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        T::compose(components, generics)
    }
}

impl<T: ToSchema> ToSchema for [T] {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        schema!(
            #[inline]
            [T]
        )
        .into()
    }
}
impl<T: ComposeSchema> ComposeSchema for [T] {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        let t_schema = generics
            .first()
            .cloned()
            .unwrap_or_else(|| T::compose(components, vec![]));
        schema::Array::new().items(t_schema).into()
    }
}

impl<T: ToSchema, const N: usize> ToSchema for [T; N] {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        schema!(
            #[inline]
            [T; N]
        )
        .into()
    }
}
impl<T: ComposeSchema, const N: usize> ComposeSchema for [T; N] {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        let t_schema = generics
            .first()
            .cloned()
            .unwrap_or_else(|| T::compose(components, vec![]));
        schema::Array::new().items(t_schema).into()
    }
}

impl<T: ToSchema> ToSchema for &[T] {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        schema!(
            #[inline]
            &[T]
        )
        .into()
    }
}
impl<T: ComposeSchema> ComposeSchema for &[T] {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        let t_schema = generics
            .first()
            .cloned()
            .unwrap_or_else(|| T::compose(components, vec![]));
        schema::Array::new().items(t_schema).into()
    }
}

impl<T: ToSchema> ToSchema for Option<T> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        schema!(#[inline] Option<T>).into()
    }
}
impl<T: ComposeSchema> ComposeSchema for Option<T> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        let t_schema = generics
            .first()
            .cloned()
            .unwrap_or_else(|| T::compose(components, vec![]));
        schema::OneOf::new()
            .item(t_schema)
            .item(schema::Object::new().schema_type(schema::BasicType::Null))
            .into()
    }
}

impl<T> ToSchema for PhantomData<T> {
    fn to_schema(_components: &mut Components) -> RefOr<schema::Schema> {
        Schema::Object(Box::default()).into()
    }
}
impl<T> ComposeSchema for PhantomData<T> {
    fn compose(
        _components: &mut Components,
        _generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        Schema::Object(Box::default()).into()
    }
}

impl<K: ToSchema, V: ToSchema> ToSchema for BTreeMap<K, V> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        schema!(#[inline]BTreeMap<K, V>).into()
    }
}
impl<K: ComposeSchema, V: ComposeSchema> ComposeSchema for BTreeMap<K, V> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        let v_schema = generics
            .get(1)
            .cloned()
            .unwrap_or_else(|| V::compose(components, vec![]));
        schema::Object::new().additional_properties(v_schema).into()
    }
}

impl<K: ToSchema, V: ToSchema> ToSchema for HashMap<K, V> {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        schema!(#[inline]HashMap<K, V>).into()
    }
}
impl<K: ComposeSchema, V: ComposeSchema> ComposeSchema for HashMap<K, V> {
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        let v_schema = generics
            .get(1)
            .cloned()
            .unwrap_or_else(|| V::compose(components, vec![]));
        schema::Object::new().additional_properties(v_schema).into()
    }
}

impl ToSchema for StatusError {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        let name = crate::naming::assign_name::<Self>(Default::default());
        let ref_or = crate::RefOr::Ref(crate::Ref::new(format!("#/components/schemas/{name}")));
        if !components.schemas.contains_key(&name) {
            components.schemas.insert(name.clone(), ref_or.clone());
            let schema = Schema::from(
                Object::new()
                    .property("code", u16::to_schema(components))
                    .required("code")
                    .required("name")
                    .property("name", String::to_schema(components))
                    .required("brief")
                    .property("brief", String::to_schema(components))
                    .required("detail")
                    .property("detail", String::to_schema(components))
                    .property("cause", String::to_schema(components)),
            );
            components.schemas.insert(name, schema);
        }
        ref_or
    }
}
impl ComposeSchema for StatusError {
    fn compose(
        components: &mut Components,
        _generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        Self::to_schema(components)
    }
}

impl ToSchema for salvo_core::Error {
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        StatusError::to_schema(components)
    }
}
impl ComposeSchema for salvo_core::Error {
    fn compose(
        components: &mut Components,
        _generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        Self::to_schema(components)
    }
}

impl<T, E> ToSchema for Result<T, E>
where
    T: ToSchema,
    E: ToSchema,
{
    fn to_schema(components: &mut Components) -> RefOr<schema::Schema> {
        let name = crate::naming::assign_name::<StatusError>(Default::default());
        let ref_or = crate::RefOr::Ref(crate::Ref::new(format!("#/components/schemas/{name}")));
        if !components.schemas.contains_key(&name) {
            components.schemas.insert(name.clone(), ref_or.clone());
            let schema = OneOf::new()
                .item(T::to_schema(components))
                .item(E::to_schema(components));
            components.schemas.insert(name, schema);
        }
        ref_or
    }
}
impl<T, E> ComposeSchema for Result<T, E>
where
    T: ComposeSchema,
    E: ComposeSchema,
{
    fn compose(
        components: &mut Components,
        generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        let t_schema = generics
            .first()
            .cloned()
            .unwrap_or_else(|| T::compose(components, vec![]));
        let e_schema = generics
            .get(1)
            .cloned()
            .unwrap_or_else(|| E::compose(components, vec![]));
        OneOf::new().item(t_schema).item(e_schema).into()
    }
}

impl ToSchema for serde_json::Value {
    fn to_schema(_components: &mut Components) -> RefOr<schema::Schema> {
        Schema::Object(Box::default()).into()
    }
}
impl ComposeSchema for serde_json::Value {
    fn compose(
        _components: &mut Components,
        _generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        Schema::Object(Box::default()).into()
    }
}

impl ToSchema for serde_json::Map<String, serde_json::Value> {
    fn to_schema(_components: &mut Components) -> RefOr<schema::Schema> {
        Schema::Object(Box::new(schema::Object::new())).into()
    }
}
impl ComposeSchema for serde_json::Map<String, serde_json::Value> {
    fn compose(
        _components: &mut Components,
        _generics: Vec<RefOr<schema::Schema>>,
    ) -> RefOr<schema::Schema> {
        Schema::Object(Box::new(schema::Object::new())).into()
    }
}

/// Trait used to convert implementing type to OpenAPI parameters.
///
/// This trait is [derivable][derive] for structs which are used to describe `path` or `query`
/// parameters. For more details of `#[derive(ToParameters)]` refer to [derive
/// documentation][derive].
///
/// # Examples
///
/// Derive [`ToParameters`] implementation. This example will fail to compile because
/// [`ToParameters`] cannot be used alone and it need to be used together with endpoint using the
/// params as well. See [derive documentation][derive] for more details.
/// ```
/// use salvo_core::prelude::*;
/// use salvo_oapi::{Components, EndpointArgRegister, Operation, ToParameters};
/// use serde::Deserialize;
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
/// # use salvo_oapi::{ToParameters, EndpointArgRegister, Components, Operation};
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
///     fn to_parameters(_components: &mut Components) -> salvo_oapi::Parameters {
///         salvo_oapi::Parameters::new()
///             .parameter(
///                 salvo_oapi::Parameter::new("id")
///                     .required(salvo_oapi::Required::True)
///                     .parameter_in(salvo_oapi::ParameterIn::Path)
///                     .description("Id of pet")
///                     .schema(
///                         salvo_oapi::Object::new()
///                             .schema_type(salvo_oapi::schema::BasicType::Integer)
///                             .format(salvo_oapi::SchemaFormat::KnownFormat(
///                                 salvo_oapi::schema::KnownFormat::Int64,
///                             )),
///                     ),
///             )
///             .parameter(
///                 salvo_oapi::Parameter::new("name")
///                     .required(salvo_oapi::Required::True)
///                     .parameter_in(salvo_oapi::ParameterIn::Query)
///                     .description("Name of pet")
///                     .schema(
///                         salvo_oapi::Object::new()
///                             .schema_type(salvo_oapi::schema::BasicType::String),
///                     ),
///             )
///     }
/// }
///
/// impl<'ex> Extractible<'ex> for PetParams {
///     fn metadata() -> &'static Metadata {
///         static METADATA: Metadata = Metadata::new("");
///         &METADATA
///     }
///     #[allow(refining_impl_trait)]
///     async fn extract(
///         req: &'ex mut Request,
///         depot: &'ex mut Depot,
///     ) -> Result<Self, salvo_core::http::ParseError> {
///         salvo_core::serde::from_request(req, depot, Self::metadata()).await
///     }
///     #[allow(refining_impl_trait)]
///     async fn extract_with_arg(
///         req: &'ex mut Request,
///         depot: &'ex mut Depot,
///         _arg: &str,
///     ) -> Result<Self, salvo_core::http::ParseError> {
///         Self::extract(req, depot).await
///     }
/// }
///
/// impl EndpointArgRegister for PetParams {
///     fn register(components: &mut Components, operation: &mut Operation, _arg: &str) {
///         operation
///             .parameters
///             .append(&mut PetParams::to_parameters(components));
///     }
/// }
/// ```
/// [derive]: derive.ToParameters.html
pub trait ToParameters<'de>: Extractible<'de> {
    /// Provide [`Vec`] of [`Parameter`]s to caller. The result is used in `salvo-oapi-macros`
    /// library to provide OpenAPI parameter information for the endpoint using the parameters.
    fn to_parameters(components: &mut Components) -> Parameters;
}

/// Trait used to give [`Parameter`] information for OpenAPI.
pub trait ToParameter {
    /// Returns a `Parameter`.
    fn to_parameter(components: &mut Components) -> Parameter;
}

/// This trait is implemented to document a type (like an enum) which can represent
/// request body, to be used in operation.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
///
/// use salvo_oapi::{
///     Components, Content, EndpointArgRegister, Operation, RequestBody, ToRequestBody, ToSchema,
/// };
/// use serde::Deserialize;
///
/// #[derive(ToSchema, Deserialize, Debug)]
/// struct MyPayload {
///     name: String,
/// }
///
/// impl ToRequestBody for MyPayload {
///     fn to_request_body(components: &mut Components) -> RequestBody {
///         RequestBody::new().add_content(
///             "application/json",
///             Content::new(MyPayload::to_schema(components)),
///         )
///     }
/// }
/// impl EndpointArgRegister for MyPayload {
///     fn register(components: &mut Components, operation: &mut Operation, _arg: &str) {
///         operation.request_body = Some(Self::to_request_body(components));
///     }
/// }
/// ```
pub trait ToRequestBody {
    /// Returns `RequestBody`.
    fn to_request_body(components: &mut Components) -> RequestBody;
}

/// This trait is implemented to document a type (like an enum) which can represent multiple
/// responses, to be used in operation.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
///
/// use salvo_oapi::{Components, RefOr, Response, Responses, ToResponses};
///
/// enum MyResponse {
///     Ok,
///     NotFound,
/// }
///
/// impl ToResponses for MyResponse {
///     fn to_responses(_components: &mut Components) -> Responses {
///         Responses::new()
///             .response("200", Response::new("Ok"))
///             .response("404", Response::new("Not Found"))
///     }
/// }
/// ```
pub trait ToResponses {
    /// Returns an ordered map of response codes to responses.
    fn to_responses(components: &mut Components) -> Responses;
}

impl<C> ToResponses for writing::Json<C>
where
    C: ToSchema,
{
    fn to_responses(components: &mut Components) -> Responses {
        Responses::new().response(
            "200",
            Response::new("Response json format data")
                .add_content("application/json", Content::new(C::to_schema(components))),
        )
    }
}

impl ToResponses for StatusError {
    fn to_responses(components: &mut Components) -> Responses {
        let mut responses = Responses::new();
        let errors = vec![
            Self::bad_request(),
            Self::unauthorized(),
            Self::payment_required(),
            Self::forbidden(),
            Self::not_found(),
            Self::method_not_allowed(),
            Self::not_acceptable(),
            Self::proxy_authentication_required(),
            Self::request_timeout(),
            Self::conflict(),
            Self::gone(),
            Self::length_required(),
            Self::precondition_failed(),
            Self::payload_too_large(),
            Self::uri_too_long(),
            Self::unsupported_media_type(),
            Self::range_not_satisfiable(),
            Self::expectation_failed(),
            Self::im_a_teapot(),
            Self::misdirected_request(),
            Self::unprocessable_entity(),
            Self::locked(),
            Self::failed_dependency(),
            Self::upgrade_required(),
            Self::precondition_required(),
            Self::too_many_requests(),
            Self::request_header_fields_toolarge(),
            Self::unavailable_for_legalreasons(),
            Self::internal_server_error(),
            Self::not_implemented(),
            Self::bad_gateway(),
            Self::service_unavailable(),
            Self::gateway_timeout(),
            Self::http_version_not_supported(),
            Self::variant_also_negotiates(),
            Self::insufficient_storage(),
            Self::loop_detected(),
            Self::not_extended(),
            Self::network_authentication_required(),
        ];
        for Self { code, brief, .. } in errors {
            responses.insert(
                code.as_str(),
                Response::new(brief).add_content(
                    "application/json",
                    Content::new(Self::to_schema(components)),
                ),
            )
        }
        responses
    }
}
impl ToResponses for salvo_core::Error {
    fn to_responses(components: &mut Components) -> Responses {
        StatusError::to_responses(components)
    }
}

/// This trait is implemented to document a type which represents a single response which can be
/// referenced or reused as a component in multiple operations.
///
/// _`ToResponse`_ trait can also be derived with [`#[derive(ToResponse)]`][derive].
///
/// # Examples
///
/// ```
/// use salvo_oapi::{Components, RefOr, Response, ToResponse};
///
/// struct MyResponse;
/// impl ToResponse for MyResponse {
///     fn to_response(_components: &mut Components) -> RefOr<Response> {
///         Response::new("My Response").into()
///     }
/// }
/// ```
///
/// [derive]: derive.ToResponse.html
pub trait ToResponse {
    /// Returns a tuple of response component name (to be referenced) to a response.
    fn to_response(components: &mut Components) -> RefOr<crate::Response>;
}

impl<C> ToResponse for writing::Json<C>
where
    C: ToSchema,
{
    fn to_response(components: &mut Components) -> RefOr<Response> {
        let schema = <C as ToSchema>::to_schema(components);
        Response::new("Response with json format data")
            .add_content("application/json", Content::new(schema))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_primitive_schema() {
        let mut components = Components::new();

        // Format expectations differ based on whether "non-strict-integers" feature is enabled.
        // With the feature: each integer type gets its own format (int8, uint8, int16, etc.)
        // Without: smaller integers collapse to int32/int64 per OpenAPI convention.
        let non_strict = cfg!(feature = "non-strict-integers");

        for (name, schema, value) in [
            (
                "i8",
                i8::to_schema(&mut components),
                if non_strict {
                    json!({"type": "integer", "format": "int8"})
                } else {
                    json!({"type": "integer", "format": "int32"})
                },
            ),
            (
                "i16",
                i16::to_schema(&mut components),
                if non_strict {
                    json!({"type": "integer", "format": "int16"})
                } else {
                    json!({"type": "integer", "format": "int32"})
                },
            ),
            (
                "i32",
                i32::to_schema(&mut components),
                json!({"type": "integer", "format": "int32"}),
            ),
            (
                "i64",
                i64::to_schema(&mut components),
                json!({"type": "integer", "format": "int64"}),
            ),
            (
                "i128",
                i128::to_schema(&mut components),
                json!({"type": "integer"}),
            ),
            (
                "isize",
                isize::to_schema(&mut components),
                json!({"type": "integer"}),
            ),
            (
                "u8",
                u8::to_schema(&mut components),
                if non_strict {
                    json!({"type": "integer", "format": "uint8", "minimum": 0})
                } else {
                    json!({"type": "integer", "format": "int32", "minimum": 0})
                },
            ),
            (
                "u16",
                u16::to_schema(&mut components),
                if non_strict {
                    json!({"type": "integer", "format": "uint16", "minimum": 0})
                } else {
                    json!({"type": "integer", "format": "int32", "minimum": 0})
                },
            ),
            (
                "u32",
                u32::to_schema(&mut components),
                if non_strict {
                    json!({"type": "integer", "format": "uint32", "minimum": 0})
                } else {
                    json!({"type": "integer", "format": "int32", "minimum": 0})
                },
            ),
            (
                "u64",
                u64::to_schema(&mut components),
                if non_strict {
                    json!({"type": "integer", "format": "uint64", "minimum": 0})
                } else {
                    json!({"type": "integer", "format": "int64", "minimum": 0})
                },
            ),
            (
                "u128",
                u128::to_schema(&mut components),
                json!({"type": "integer", "minimum": 0}),
            ),
            (
                "usize",
                usize::to_schema(&mut components),
                json!({"type": "integer", "minimum": 0}),
            ),
            (
                "bool",
                bool::to_schema(&mut components),
                json!({"type": "boolean"}),
            ),
            (
                "str",
                str::to_schema(&mut components),
                json!({"type": "string"}),
            ),
            (
                "String",
                String::to_schema(&mut components),
                json!({"type": "string"}),
            ),
            (
                "char",
                char::to_schema(&mut components),
                json!({"type": "string"}),
            ),
            (
                "f32",
                f32::to_schema(&mut components),
                json!({"type": "number", "format": "float"}),
            ),
            (
                "f64",
                f64::to_schema(&mut components),
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
