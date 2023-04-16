//! This is **private** salvo_oapi codegen library and is not used alone.
//!
//! The library contains macro implementations for salvo_oapi library. Content
//! of the library documentation is available through **salvo_oapi** library itself.
//! Consider browsing via the **salvo_oapi** crate so all links will work correctly.

#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::ops::Deref;

use component::schema::Schema;
use doc_comment::CommentAttributes;

use component::into_params::IntoParams;
use proc_macro::TokenStream;
use proc_macro_error::{abort, proc_macro_error};
use quote::{quote, ToTokens, TokenStreamExt};

use proc_macro2::{Group, Ident, Punct, Span, TokenStream as TokenStream2};
use syn::{
    bracketed,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    token::Bracket,
    DeriveInput, ExprPath, ItemFn, Lit, LitStr, Member, Token,
};

mod component;
mod doc_comment;
mod openapi;
mod path;
mod schema_type;
mod security_requirement;
mod shared;
pub(crate) use shared::*;

use crate::path::{Path, PathAttr};

use self::{
    component::{
        features::{self, Feature},
        ComponentSchema, ComponentSchemaProps, TypeTree,
    },
    path::response::derive::{IntoResponses, ToResponse},
};

#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "debug", derive(Debug))]
struct ArgValue {
    name: String,
    original_name: String,
}

#[proc_macro_error]
#[proc_macro_derive(ToSchema, attributes(schema, aliases))]
/// Generate reusable OpenAPI schema to be used
/// together with [`OpenApi`][openapi_derive].
///
/// This is `#[derive]` implementation for [`ToSchema`][to_schema] trait. The macro accepts one
/// `schema`
/// attribute optionally which can be used to enhance generated documentation. The attribute can be placed
/// at item level or field level in struct and enums. Currently placing this attribute to unnamed field does
/// not have any effect.
///
/// You can use the Rust's own `#[deprecated]` attribute on any struct, enum or field to mark it as deprecated and it will
/// reflect to the generated OpenAPI spec.
///
/// `#[deprecated]` attribute supports adding additional details such as a reason and or since version but this is is not supported in
/// OpenAPI. OpenAPI has only a boolean flag to determine deprecation. While it is totally okay to declare deprecated with reason
/// `#[deprecated  = "There is better way to do this"]` the reason would not render in OpenAPI spec.
///
/// Doc comments on fields will resolve to field descriptions in generated OpenAPI doc. On struct
/// level doc comments will resolve to object descriptions.
///
/// ```
/// /// This is a pet
/// #[derive(salvo_oapi::ToSchema)]
/// struct Pet {
///     /// Name for your pet
///     name: String,
/// }
/// ```
///
/// # Struct Optional Configuration Options for `#[schema(...)]`
/// * `example = ...` Can be _`json!(...)`_. _`json!(...)`_ should be something that
///   _`serde_json::json!`_ can parse as a _`serde_json::Value`_.
/// * `xml(...)` Can be used to define [`Xml`][xml] object properties applicable to Structs.
/// * `title = ...` Literal string value. Can be used to define title for struct in OpenAPI
///   document. Some OpenAPI code generation libraries also use this field as a name for the
///   struct.
/// * `rename_all = ...` Supports same syntax as _serde_ _`rename_all`_ attribute. Will rename all fields
///   of the structs accordingly. If both _serde_ `rename_all` and _schema_ _`rename_all`_ are defined
///   __serde__ will take precedence.
/// * `as = ...` Can be used to define alternative path and name for the schema what will be used in
///   the OpenAPI. E.g _`as = path::to::Pet`_. This would make the schema appear in the generated
///   OpenAPI spec as _`path.to.Pet`_.
/// * `default` Can be used to populate default values on all fields using the struct's
///   [`Default`](std::default::Default) implementation.
///
/// # Enum Optional Configuration Options for `#[schema(...)]`
/// * `example = ...` Can be method reference or _`json!(...)`_.
/// * `default = ...` Can be method reference or _`json!(...)`_.
/// * `title = ...` Literal string value. Can be used to define title for enum in OpenAPI
///   document. Some OpenAPI code generation libraries also use this field as a name for the
///   enum. __Note!__  ___Complex enum (enum with other than unit variants) does not support title!___
/// * `rename_all = ...` Supports same syntax as _serde_ _`rename_all`_ attribute. Will rename all
///   variants of the enum accordingly. If both _serde_ `rename_all` and _schema_ _`rename_all`_
///   are defined __serde__ will take precedence.
/// * `as = ...` Can be used to define alternative path and name for the schema what will be used in
///   the OpenAPI. E.g _`as = path::to::Pet`_. This would make the schema appear in the generated
///   OpenAPI spec as _`path.to.Pet`_.
///
/// # Enum Variant Optional Configuration Options for `#[schema(...)]`
/// Supports all variant specific configuration options e.g. if variant is _`UnnamedStruct`_ then
/// unnamed struct type configuration options are supported.
///
/// In addition to the variant type specific configuration options enum variants support custom
/// _`rename`_ attribute. It behaves similarly to serde's _`rename`_ attribute. If both _serde_
/// _`rename`_ and _schema_ _`rename`_ are defined __serde__ will take precedence.
///
/// # Unnamed Field Struct Optional Configuration Options for `#[schema(...)]`
/// * `example = ...` Can be method reference or _`json!(...)`_.
/// * `default = ...` Can be method reference or _`json!(...)`_. If no value is specified, and the struct has
///   only one field, the field's default value in the schema will be set from the struct's
///   [`Default`](std::default::Default) implementation.
/// * `format = ...` May either be variant of the [`KnownFormat`][known_format] enum, or otherwise
///   an open value as a string. By default the format is derived from the type of the property
///   according OpenApi spec.
/// * `value_type = ...` Can be used to override default type derived from type of the field used in OpenAPI spec.
///   This is useful in cases where the default type does not correspond to the actual type e.g. when
///   any third-party types are used which are not [`ToSchema`][to_schema]s nor [`primitive` types][primitive].
///    Value can be any Rust type what normally could be used to serialize to JSON or custom type such as _`Object`_.
///    _`Object`_ will be rendered as generic OpenAPI object _(`type: object`)_.
/// * `title = ...` Literal string value. Can be used to define title for struct in OpenAPI
///   document. Some OpenAPI code generation libraries also use this field as a name for the
///   struct.
/// * `as = ...` Can be used to define alternative path and name for the schema what will be used in
///   the OpenAPI. E.g _`as = path::to::Pet`_. This would make the schema appear in the generated
///   OpenAPI spec as _`path.to.Pet`_.
///
/// # Named Fields Optional Configuration Options for `#[schema(...)]`
/// * `example = ...` Can be method reference or _`json!(...)`_.
/// * `default = ...` Can be method reference or _`json!(...)`_.
/// * `format = ...` May either be variant of the [`KnownFormat`][known_format] enum, or otherwise
///   an open value as a string. By default the format is derived from the type of the property
///   according OpenApi spec.
/// * `write_only` Defines property is only used in **write** operations *POST,PUT,PATCH* but not in *GET*
/// * `read_only` Defines property is only used in **read** operations *GET* but not in *POST,PUT,PATCH*
/// * `xml(...)` Can be used to define [`Xml`][xml] object properties applicable to named fields.
///    See configuration options at xml attributes of [`ToSchema`][to_schema_xml]
/// * `value_type = ...` Can be used to override default type derived from type of the field used in OpenAPI spec.
///   This is useful in cases where the default type does not correspond to the actual type e.g. when
///   any third-party types are used which are not [`ToSchema`][to_schema]s nor [`primitive` types][primitive].
///    Value can be any Rust type what normally could be used to serialize to JSON or custom type such as _`Object`_.
///    _`Object`_ will be rendered as generic OpenAPI object _(`type: object`)_.
/// * `inline` If the type of this field implements [`ToSchema`][to_schema], then the schema definition
///   will be inlined. **warning:** Don't use this for recursive data types!
/// * `required = ...` Can be used to enforce required status for the field. [See
///   rules][derive@ToSchema#field-nullability-and-required-rules]
/// * `nullable` Defines property is nullable (note this is different to non-required).
/// * `rename = ...` Supports same syntax as _serde_ _`rename`_ attribute. Will rename field
///   accordingly. If both _serde_ `rename` and _schema_ _`rename`_ are defined __serde__ will take
///   precedence.
/// * `multiple_of = ...` Can be used to define multiplier for a value. Value is considered valid
///   division will result an `integer`. Value must be strictly above _`0`_.
/// * `maximum = ...` Can be used to define inclusive upper bound to a `number` value.
/// * `minimum = ...` Can be used to define inclusive lower bound to a `number` value.
/// * `exclusive_maximum = ...` Can be used to define exclusive upper bound to a `number` value.
/// * `exclusive_minimum = ...` Can be used to define exclusive lower bound to a `number` value.
/// * `max_length = ...` Can be used to define maximum length for `string` types.
/// * `min_length = ...` Can be used to define minimum length for `string` types.
/// * `pattern = ...` Can be used to define valid regular expression in _ECMA-262_ dialect the field value must match.
/// * `max_items = ...` Can be used to define maximum items allowed for `array` fields. Value must
///   be non-negative integer.
/// * `min_items = ...` Can be used to define minimum items allowed for `array` fields. Value must
///   be non-negative integer.
/// * `with_schema = ...` Use _`schema`_ created by provided function reference instead of the
///   default derived _`schema`_. The function must match to `fn() -> Into<RefOr<Schema>>`. It does
///   not accept arguments and must return anything that can be converted into `RefOr<Schema>`.
/// * `additional_properties = ...` Can be used to define free form types for maps such as
///   [`HashMap`](std::collections::HashMap) and [`BTreeMap`](std::collections::BTreeMap).
///   Free form type enables use of arbitrary types within map values.
///   Supports formats _`additional_properties`_ and _`additional_properties = true`_.
///
/// #### Field nullability and required rules
///
/// Field is considered _`required`_ if
/// * it is not `Option` field
/// * and it does not have _`skip_serializing_if`_ property
/// * and it does not have _`serde_with`_ _[`double_option`](https://docs.rs/serde_with/latest/serde_with/rust/double_option/index.html)_
/// * and it does not have default value provided with serde _`default`_
///   attribute
///
/// Field is considered _`nullable`_ when field type is _`Option`_.
///
/// ## Xml attribute Configuration Options
///
/// * `xml(name = "...")` Will set name for property or type.
/// * `xml(namespace = "...")` Will set namespace for xml element which needs to be valid uri.
/// * `xml(prefix = "...")` Will set prefix for name.
/// * `xml(attribute)` Will translate property to xml attribute instead of xml element.
/// * `xml(wrapped)` Will make wrapped xml element.
/// * `xml(wrapped(name = "wrap_name"))` Will override the wrapper elements name.
///
/// See [`Xml`][xml] for more details.
///
/// # Partial `#[serde(...)]` attributes support
///
/// ToSchema derive has partial support for [serde attributes]. These supported attributes will reflect to the
/// generated OpenAPI doc. For example if _`#[serde(skip)]`_ is defined the attribute will not show up in the OpenAPI spec at all since it will not never
/// be serialized anyway. Similarly the _`rename`_ and _`rename_all`_ will reflect to the generated OpenAPI doc.
///
/// * `rename_all = "..."` Supported at the container level.
/// * `rename = "..."` Supported **only** at the field or variant level.
/// * `skip = "..."` Supported  **only** at the field or variant level.
/// * `skip_serializing = "..."` Supported  **only** at the field or variant level.
/// * `skip_serializing_if = "..."` Supported  **only** at the field level.
/// * `with = ...` Supported **only at field level.**
/// * `tag = "..."` Supported at the container level. `tag` attribute works as a [discriminator field][discriminator] for an enum.
/// * `content = "..."` Supported at the container level, allows [adjacently-tagged enums](https://serde.rs/enum-representations.html#adjacently-tagged).
///   This attribute requires that a `tag` is present, otherwise serde will trigger a compile-time
///   failure.
/// * `untagged` Supported at the container level. Allows [untagged
/// enum representation](https://serde.rs/enum-representations.html#untagged).
/// * `default` Supported at the container level and field level according to [serde attributes].
/// * `flatten` Supported at the field level.
///
/// Other _`serde`_ attributes works as is but does not have any effect on the generated OpenAPI doc.
///
/// **Note!** `tag` attribute has some limitations like it cannot be used
/// with **unnamed field structs** and **tuple types**.  See more at
/// [enum representation docs](https://serde.rs/enum-representations.html).
///
/// **Note!** `with` attribute is used in tandem with [serde_with](https://github.com/jonasbb/serde_with) to recognize
/// _[`double_option`](https://docs.rs/serde_with/latest/serde_with/rust/double_option/index.html)_ from **field value**.
/// _`double_option`_ is **only** supported attribute from _`serde_with`_ crate.
///
/// ```
/// # use serde::Serialize;
/// # use salvo_oapi::ToSchema;
/// #[derive(Serialize, ToSchema)]
/// struct Foo(String);
///
/// #[derive(Serialize, ToSchema)]
/// #[serde(rename_all = "camelCase")]
/// enum Bar {
///     UnitValue,
///     #[serde(rename_all = "camelCase")]
///     NamedFields {
///         #[serde(rename = "id")]
///         named_id: &'static str,
///         name_list: Option<Vec<String>>
///     },
///     UnnamedFields(Foo),
///     #[serde(skip)]
///     SkipMe,
/// }
/// ```
///
/// _**Add custom `tag` to change JSON representation to be internally tagged.**_
/// ```
/// # use serde::Serialize;
/// # use salvo_oapi::ToSchema;
/// #[derive(Serialize, ToSchema)]
/// struct Foo(String);
///
/// #[derive(Serialize, ToSchema)]
/// #[serde(tag = "tag")]
/// enum Bar {
///     UnitValue,
///     NamedFields {
///         id: &'static str,
///         names: Option<Vec<String>>
///     },
/// }
/// ```
///
/// _**Add serde `default` attribute for MyValue struct. Similarly `default` could be added to
/// individual fields as well. If `default` is given the field's affected will be treated
/// as optional.**_
/// ```
///  #[derive(salvo_oapi::ToSchema, serde::Deserialize, Default)]
///  #[serde(default)]
///  struct MyValue {
///      field: String
///  }
/// ```
///
/// # `#[repr(...)]` attribute support
///
/// [Serde repr](https://github.com/dtolnay/serde-repr) allows field-less enums be represented by
/// their numeric value.
///
/// * `repr(u*)` for unsigned integer.
/// * `repr(i*)` for signed integer.
///
/// **Supported schema attributes**
///
/// * `example = ...` Can be method reference or _`json!(...)`_.
/// * `default = ...` Can be method reference or _`json!(...)`_.
/// * `title = ...` Literal string value. Can be used to define title for enum in OpenAPI
///   document. Some OpenAPI code generation libraries also use this field as a name for the
///   enum. __Note!__  ___Complex enum (enum with other than unit variants) does not support title!___
/// * `as = ...` Can be used to define alternative path and name for the schema what will be used in
///   the OpenAPI. E.g _`as = path::to::Pet`_. This would make the schema appear in the generated
///   OpenAPI spec as _`path.to.Pet`_.
///
/// _**Create enum with numeric values.**_
/// ```
/// # use salvo_oapi::ToSchema;
/// #[derive(ToSchema)]
/// #[repr(u8)]
/// #[schema(default = default_value, example = 2)]
/// enum Mode {
///     One = 1,
///     Two,
///  }
///
/// fn default_value() -> u8 {
///     1
/// }
/// ```
///
/// _**You can use `skip` and `tag` attributes from serde.**_
/// ```
/// # use salvo_oapi::ToSchema;
/// #[derive(ToSchema, serde::Serialize)]
/// #[repr(i8)]
/// #[serde(tag = "code")]
/// enum ExitCode {
///     Error = -1,
///     #[serde(skip)]
///     Unknown = 0,
///     Ok = 1,
///  }
/// ```
///
/// # Generic schemas with aliases
///
/// Schemas can also be generic which allows reusing types. This enables certain behaviour patters
/// where super type declares common code for type aliases.
///
/// In this example we have common `Status` type which accepts one generic type. It is then defined
/// with `#[aliases(...)]` that it is going to be used with [`String`](std::string::String) and [`i32`] values.
/// The generic argument could also be another [`ToSchema`][to_schema] as well.
/// ```
/// # use salvo_oapi::{ToSchema, OpenApi};
/// #[derive(ToSchema)]
/// #[aliases(StatusMessage = Status<String>, StatusNumber = Status<i32>)]
/// struct Status<T> {
///     value: T
/// }
///
/// #[derive(OpenApi)]
/// #[openapi(
///     components(schemas(StatusMessage, StatusNumber))
/// )]
/// struct ApiDoc;
/// ```
///
/// The `#[aliases(...)]` is just syntactic sugar and will create Rust [type aliases](https://doc.rust-lang.org/reference/items/type-aliases.html)
/// behind the scenes which then can be later referenced anywhere in code.
///
/// **Note!** You should never register generic type itself in `components(...)` so according above example `Status<...>` should not be registered
/// because it will not render the type correctly and will cause an error in generated OpenAPI spec.
///
/// # Examples
///
/// _**Simple example of a Pet with descriptions and object level example.**_
/// ```
/// # use salvo_oapi::ToSchema;
/// /// This is a pet.
/// #[derive(ToSchema)]
/// #[schema(example = json!({"name": "bob the cat", "id": 0}))]
/// struct Pet {
///     /// Unique id of a pet.
///     id: u64,
///     /// Name of a pet.
///     name: String,
///     /// Age of a pet if known.
///     age: Option<i32>,
/// }
/// ```
///
/// _**The `schema` attribute can also be placed at field level as follows.**_
/// ```
/// # use salvo_oapi::ToSchema;
/// #[derive(ToSchema)]
/// struct Pet {
///     #[schema(example = 1, default = 0)]
///     id: u64,
///     name: String,
///     age: Option<i32>,
/// }
/// ```
///
/// _**You can also use method reference for attribute values.**_
/// ```
/// # use salvo_oapi::ToSchema;
/// #[derive(ToSchema)]
/// struct Pet {
///     #[schema(example = u64::default, default = u64::default)]
///     id: u64,
///     #[schema(default = default_name)]
///     name: String,
///     age: Option<i32>,
/// }
///
/// fn default_name() -> String {
///     "bob".to_string()
/// }
/// ```
///
/// _**For enums and unnamed field structs you can define `schema` at type level.**_
/// ```
/// # use salvo_oapi::ToSchema;
/// #[derive(ToSchema)]
/// #[schema(example = "Bus")]
/// enum VehicleType {
///     Rocket, Car, Bus, Submarine
/// }
/// ```
///
/// _**Also you write complex enum combining all above types.**_
/// ```
/// # use salvo_oapi::ToSchema;
/// #[derive(ToSchema)]
/// enum ErrorResponse {
///     InvalidCredentials,
///     #[schema(default = String::default, example = "Pet not found")]
///     NotFound(String),
///     System {
///         #[schema(example = "Unknown system failure")]
///         details: String,
///     }
/// }
/// ```
///
/// _**It is possible to specify the title of each variant to help generators create named structures.**_
/// ```
/// # use salvo_oapi::ToSchema;
/// #[derive(ToSchema)]
/// enum ErrorResponse {
///     #[schema(title = "InvalidCredentials")]
///     InvalidCredentials,
///     #[schema(title = "NotFound")]
///     NotFound(String),
/// }
/// ```
///
/// _**Use `xml` attribute to manipulate xml output.**_
/// ```
/// # use salvo_oapi::ToSchema;
/// #[derive(ToSchema)]
/// #[schema(xml(name = "user", prefix = "u", namespace = "https://user.xml.schema.test"))]
/// struct User {
///     #[schema(xml(attribute, prefix = "u"))]
///     id: i64,
///     #[schema(xml(name = "user_name", prefix = "u"))]
///     username: String,
///     #[schema(xml(wrapped(name = "linkList"), name = "link"))]
///     links: Vec<String>,
///     #[schema(xml(wrapped, name = "photo_url"))]
///     photos_urls: Vec<String>
/// }
/// ```
///
/// _**Use of Rust's own `#[deprecated]` attribute will reflect to generated OpenAPI spec.**_
/// ```
/// # use salvo_oapi::ToSchema;
/// #[derive(ToSchema)]
/// #[deprecated]
/// struct User {
///     id: i64,
///     username: String,
///     links: Vec<String>,
///     #[deprecated]
///     photos_urls: Vec<String>
/// }
/// ```
///
/// _**Enforce type being used in OpenAPI spec to [`String`] with `value_type` and set format to octet stream
/// with [`SchemaFormat::KnownFormat(KnownFormat::Binary)`][binary].**_
/// ```
/// # use salvo_oapi::ToSchema;
/// #[derive(ToSchema)]
/// struct Post {
///     id: i32,
///     #[schema(value_type = String, format = Binary)]
///     value: Vec<u8>,
/// }
/// ```
///
/// _**Enforce type being used in OpenAPI spec to [`String`] with `value_type` option.**_
/// ```
/// # use salvo_oapi::ToSchema;
/// #[derive(ToSchema)]
/// #[schema(value_type = String)]
/// struct Value(i64);
/// ```
///
/// _**Override the `Bar` reference with a `custom::NewBar` reference.**_
/// ```
/// # use salvo_oapi::ToSchema;
/// #  mod custom {
/// #      struct NewBar;
/// #  }
/// #
/// # struct Bar;
/// #[derive(ToSchema)]
/// struct Value {
///     #[schema(value_type = custom::NewBar)]
///     field: Bar,
/// };
/// ```
///
/// _**Use a virtual `Object` type to render generic `object` _(`type: object`)_ in OpenAPI spec.**_
/// ```
/// # use salvo_oapi::ToSchema;
/// # mod custom {
/// #    struct NewBar;
/// # }
/// #
/// # struct Bar;
/// #[derive(ToSchema)]
/// struct Value {
///     #[schema(value_type = Object)]
///     field: Bar,
/// };
/// ```
///
/// _**Serde `rename` / `rename_all` will take precedence over schema `rename` / `rename_all`.**_
/// ```
/// #[derive(salvo_oapi::ToSchema, serde::Deserialize)]
/// #[serde(rename_all = "lowercase")]
/// #[schema(rename_all = "UPPERCASE")]
/// enum Random {
///     #[serde(rename = "string_value")]
///     #[schema(rename = "custom_value")]
///     String(String),
///
///     Number {
///         id: i32,
///     }
/// }
/// ```
///
/// _**Add `title` to the enum.**_
/// ```
/// #[derive(salvo_oapi::ToSchema)]
/// #[schema(title = "UserType")]
/// enum UserType {
///     Admin,
///     Moderator,
///     User,
/// }
/// ```
///
/// _**Example with validation attributes.**_
/// ```
/// #[derive(salvo_oapi::ToSchema)]
/// struct Item {
///     #[schema(maximum = 10, minimum = 5, multiple_of = 2.5)]
///     id: i32,
///     #[schema(max_length = 10, min_length = 5, pattern = "[a-z]*")]
///     value: String,
///     #[schema(max_items = 5, min_items = 1)]
///     items: Vec<String>,
/// }
/// ````
///
/// _**Use `schema_with` to manually implement schema for a field.**_
/// ```
/// # use salvo_oapi::openapi::schema::{Object, ObjectBuilder};
/// fn custom_type() -> Object {
///     ObjectBuilder::new()
///         .schema_type(salvo_oapi::openapi::SchemaType::String)
///         .format(Some(salvo_oapi::openapi::SchemaFormat::Custom(
///             "email".to_string(),
///         )))
///         .description(Some("this is the description"))
///         .build()
/// }
///
/// #[derive(salvo_oapi::ToSchema)]
/// struct Value {
///     #[schema(schema_with = custom_type)]
///     id: String,
/// }
/// ```
///
/// _**Use `as` attribute to change the name and the path of the schema in the generated OpenAPI
/// spec.**_
/// ```
///  #[derive(salvo_oapi::ToSchema)]
///  #[schema(as = api::models::person::Person)]
///  struct Person {
///      name: String,
///  }
/// ```
///
/// More examples for _`value_type`_ in [`IntoParams` derive docs][into_params].
///
/// [to_schema]: trait.ToSchema.html
/// [known_format]: openapi/schema/enum.KnownFormat.html
/// [binary]: openapi/schema/enum.KnownFormat.html#variant.Binary
/// [xml]: openapi/xml/struct.Xml.html
/// [into_params]: derive.IntoParams.html
/// [primitive]: https://doc.rust-lang.org/std/primitive/index.html
/// [serde attributes]: https://serde.rs/attributes.html
/// [discriminator]: openapi/schema/struct.Discriminator.html
/// [enum_schema]: derive.ToSchema.html#enum-optional-configuration-options-for-schema
/// [openapi_derive]: derive.OpenApi.html
/// [to_schema_xml]: macro@ToSchema#xml-attribute-configuration-options
pub fn derive_to_schema(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        data,
        generics,
        vis,
    } = syn::parse_macro_input!(input);

    let schema = Schema::new(&data, &attrs, &ident, &generics, &vis);
    schema.to_token_stream().into()
}

#[proc_macro_error]
#[proc_macro_attribute]
/// Path attribute macro implements OpenAPI path for the decorated function.
///
/// This is a `#[derive]` implementation for [`Path`][path] trait. Macro accepts set of attributes that can
/// be used to configure and override default values what are resolved automatically.
///
/// You can use the Rust's own `#[deprecated]` attribute on functions to mark it as deprecated and it will
/// reflect to the generated OpenAPI spec. Only **parameters** has a special **deprecated** attribute to define them as deprecated.
///
/// `#[deprecated]` attribute supports adding additional details such as a reason and or since version but this is is not supported in
/// OpenAPI. OpenAPI has only a boolean flag to determine deprecation. While it is totally okay to declare deprecated with reason
/// `#[deprecated  = "There is better way to do this"]` the reason would not render in OpenAPI spec.
///
/// Doc comment at decorated function will be used for _`description`_ and _`summary`_ of the path.
/// First line of the doc comment will be used as the _`summary`_ and the whole doc comment will be
/// used as _`description`_.
/// ```
/// /// This is a summary of the operation
/// ///
/// /// All lines of the doc comment will be included to operation description.
/// #[salvo_oapi::path(get, path = "/operation")]
/// fn operation() {}
/// ```
///
/// # Path Attributes
///
/// * `operation` _**Must be first parameter!**_ Accepted values are known http operations such as
///   _`get, post, put, delete, head, options, connect, patch, trace`_.
///
/// * `path = "..."` Must be OpenAPI format compatible str with arguments withing curly braces. E.g _`{id}`_
///
/// * `operation_id = ...` Unique operation id for the endpoint. By default this is mapped to function name.
///   The operation_id can be any valid expression (e.g. string literals, macro invocations, variables) so long
///   as its result can be converted to a `String` using `String::from`.
///
/// * `context_path = "..."` Can add optional scope for **path**. The **context_path** will be prepended to beginning of **path**.
///   This is particularly useful when **path** does not contain the full path to the endpoint. For example if web framework
///   allows operation to be defined under some context path or scope which does not reflect to the resolved path then this
///   **context_path** can become handy to alter the path.
///
/// * `tag = "..."` Can be used to group operations. Operations with same tag are grouped together. By default
///   this is derived from the handler that is given to [`OpenApi`][openapi]. If derive results empty str
///   then default value _`crate`_ is used instead.
///
/// * `request_body = ... | request_body(...)` Defining request body indicates that the request is expecting request body within
///   the performed request.
///
/// * `responses(...)` Slice of responses the endpoint is going to possibly return to the caller.
///
/// * `params(...)` Slice of params that the endpoint accepts.
///
/// * `security(...)` List of [`SecurityRequirement`][security]s local to the path operation.
///
/// # Request Body Attributes
///
/// **Simple format definition by `request_body = ...`**
/// * _`request_body = Type`_, _`request_body = inline(Type)`_ or _`request_body = ref("...")`_.
///   The given _`Type`_ can be any Rust type that is JSON parseable. It can be Option, Vec or Map etc.
///   With _`inline(...)`_ the schema will be inlined instead of a referenced which is the default for
///   [`ToSchema`][to_schema] types. _`ref("./external.json")`_ can be used to reference external
///   json file for body schema. **Note!** Sapid does **not** guarantee that free form _`ref`_ is accessbile via
///   OpenAPI doc or Swagger UI, users are eligible to make these guarantees.
///
/// **Advanced format definition by `request_body(...)`**
/// * `content = ...` Can be _`content = Type`_, _`content = inline(Type)`_ or _`content = ref("...")`_. The
///   given _`Type`_ can be any Rust type that is JSON parseable. It can be Option, Vec
///   or Map etc. With _`inline(...)`_ the schema will be inlined instead of a referenced
///   which is the default for [`ToSchema`][to_schema] types. _`ref("./external.json")`_
///   can be used to reference external json file for body schema. **Note!** Sapid does **not** guarantee
///   that free form _`ref`_ is accessible via OpenAPI doc or Swagger UI, users are eligible
///   to make these guarantees.
///
/// * `description = "..."` Define the description for the request body object as str.
///
/// * `content_type = "..."` Can be used to override the default behavior of auto resolving the content type
///   from the `content` attribute. If defined the value should be valid content type such as
///   _`application/json`_. By default the content type is _`text/plain`_ for
///   [primitive Rust types][primitive], `application/octet-stream` for _`[u8]`_ and
///   _`application/json`_ for struct and complex enum types.
///
/// * `example = ...` Can be _`json!(...)`_. _`json!(...)`_ should be something that
///   _`serde_json::json!`_ can parse as a _`serde_json::Value`_.
///
/// * `examples(...)` Define multiple examples for single request body. This attribute is mutually
///   exclusive to the _`example`_ attribute and if both are defined this will override the _`example`_.
///   This has same syntax as _`examples(...)`_ in [Response Attributes](#response-attributes)
///   _examples(...)_
///
/// _**Example request body definitions.**_
/// ```text
///  request_body(content = String, description = "Xml as string request", content_type = "text/xml"),
///  request_body = Pet,
///  request_body = Option<[Pet]>,
/// ```
///
/// # Response Attributes
///
/// * `status = ...` Is either a valid http status code integer. E.g. _`200`_ or a string value representing
///   a range such as _`"4XX"`_ or `"default"` or a valid _`http::status::StatusCode`_.
///   _`StatusCode`_ can either be use path to the status code or _status code_ constant directly.
///
/// * `description = "..."` Define description for the response as str.
///
/// * `body = ...` Optional response body object type. When left empty response does not expect to send any
///   response body. Can be _`body = Type`_, _`body = inline(Type)`_, or _`body = ref("...")`_.
///   The given _`Type`_ can be any Rust type that is JSON parseable. It can be Option, Vec or Map etc.
///   With _`inline(...)`_ the schema will be inlined instead of a referenced which is the default for
///   [`ToSchema`][to_schema] types. _`ref("./external.json")`_
///   can be used to reference external json file for body schema. **Note!** Sapid does **not** guarantee
///   that free form _`ref`_ is accessible via OpenAPI doc or Swagger UI, users are eligible
///   to make these guarantees.
///
/// * `content_type = "..." | content_type = [...]` Can be used to override the default behavior of auto resolving the content type
///   from the `body` attribute. If defined the value should be valid content type such as
///   _`application/json`_. By default the content type is _`text/plain`_ for
///   [primitive Rust types][primitive], `application/octet-stream` for _`[u8]`_ and
///   _`application/json`_ for struct and complex enum types.
///   Content type can also be slice of **content_type** values if the endpoint support returning multiple
///  response content types. E.g _`["application/json", "text/xml"]`_ would indicate that endpoint can return both
///  _`json`_ and _`xml`_ formats. **The order** of the content types define the default example show first in
///  the Swagger UI. Swagger UI wil use the first _`content_type`_ value as a default example.
///
/// * `headers(...)` Slice of response headers that are returned back to a caller.
///
/// * `example = ...` Can be _`json!(...)`_. _`json!(...)`_ should be something that
///   _`serde_json::json!`_ can parse as a _`serde_json::Value`_.
///
/// * `response = ...` Type what implements [`ToResponse`][to_response_trait] trait. This can alternatively be used to
///    define response attributes. _`response`_ attribute cannot co-exist with other than _`status`_ attribute.
///
/// * `content((...), (...))` Can be used to define multiple return types for single response status. Supported format for single
///   _content_ is `(content_type = response_body, example = "...", examples(...))`. _`example`_
///   and _`examples`_ are optional arguments. Examples attribute behaves exactly same way as in
///   the response and is mutually exclusive with the example attribute.
///
/// * `examples(...)` Define multiple examples for single response. This attribute is mutually
///   exclusive to the _`example`_ attribute and if both are defined this will override the _`example`_.
///     * `name = ...` This is first attribute and value must be literal string.
///     * `summary = ...` Short description of example. Value must be literal string.
///     * `description = ...` Long description of example. Attribute supports markdown for rich text
///       representation. Value must be literal string.
///     * `value = ...` Example value. It must be _`json!(...)`_. _`json!(...)`_ should be something that
///       _`serde_json::json!`_ can parse as a _`serde_json::Value`_.
///     * `external_value = ...` Define URI to literal example value. This is mutually exclusive to
///       the _`value`_ attribute. Value must be literal string.
///
///      _**Example of example definition.**_
///     ```text
///      ("John" = (summary = "This is John", value = json!({"name": "John"})))
///     ```
///
/// **Minimal response format:**
/// ```text
/// responses(
///     (status = 200, description = "success response"),
///     (status = 404, description = "resource missing"),
///     (status = "5XX", description = "server error"),
///     (status = StatusCode::INTERNAL_SERVER_ERROR, description = "internal server error"),
///     (status = IM_A_TEAPOT, description = "happy easter")
/// )
/// ```
///
/// **More complete Response:**
/// ```text
/// responses(
///     (status = 200, description = "Success response", body = Pet, content_type = "application/json",
///         headers(...),
///         example = json!({"id": 1, "name": "bob the cat"})
///     )
/// )
/// ```
///
/// **Response with multiple response content types:**
/// ```text
/// responses(
///     (status = 200, description = "Success response", body = Pet, content_type = ["application/json", "text/xml"])
/// )
/// ```
///
/// **Multiple response return types with _`content(...)`_ attribute:**
///
/// _**Define multiple response return types for single response status with their own example.**_
/// ```text
/// responses(
///    (status = 200, content(
///            ("application/vnd.user.v1+json" = User, example = json!(User {id: "id".to_string()})),
///            ("application/vnd.user.v2+json" = User2, example = json!(User2 {id: 2}))
///        )
///    )
/// )
/// ```
///
/// ### Using `ToResponse` for reusable responses
///
/// _**`ReusableResponse` must be a type that implements [`ToResponse`][to_response_trait].**_
/// ```text
/// responses(
///     (status = 200, response = ReusableResponse)
/// )
/// ```
///
/// _**[`ToResponse`][to_response_trait] can also be inlined to the responses map.**_
/// ```text
/// responses(
///     (status = 200, response = inline(ReusableResponse))
/// )
/// ```
///
/// ## Responses from `IntoResponses`
///
/// _**Responses for a path can be specified with one or more types that implement
/// [`IntoResponses`][into_responses_trait].**_
/// ```text
/// responses(MyResponse)
/// ```
///
/// # Response Header Attributes
///
/// * `name` Name of the header. E.g. _`x-csrf-token`_
///
/// * `type` Additional type of the header value. Can be `Type` or `inline(Type)`.
///   The given _`Type`_ can be any Rust type that is JSON parseable. It can be Option, Vec or Map etc.
///   With _`inline(...)`_ the schema will be inlined instead of a referenced which is the default for
///   [`ToSchema`][to_schema] types. **Reminder!** It's up to the user to use valid type for the
///   response header.
///
/// * `description = "..."` Can be used to define optional description for the response header as str.
///
/// **Header supported formats:**
///
/// ```text
/// ("x-csrf-token"),
/// ("x-csrf-token" = String, description = "New csrf token"),
/// ```
///
/// # Params Attributes
///
/// The list of attributes inside the `params(...)` attribute can take two forms: [Tuples](#tuples) or [IntoParams
/// Type](#intoparams-type).
///
/// ## Tuples
///
/// In the tuples format, parameters are specified using the following attributes inside a list of
/// tuples separated by commas:
///
/// * `name` _**Must be the first argument**_. Define the name for parameter.
///
/// * `parameter_type` Define possible type for the parameter. Can be `Type` or `inline(Type)`.
///   The given _`Type`_ can be any Rust type that is JSON parseable. It can be Option, Vec or Map etc.
///   With _`inline(...)`_ the schema will be inlined instead of a referenced which is the default for
///   [`ToSchema`][to_schema] types. Parameter type is placed after `name` with
///   equals sign E.g. _`"id" = String`_
///
/// * `in` _**Must be placed after name or parameter_type**_. Define the place of the parameter.
///   This must be one of the variants of [`openapi::path::ParameterIn`][in_enum].
///   E.g. _`Path, Query, Header, Cookie`_
///
/// * `deprecated` Define whether the parameter is deprecated or not. Can optionally be defined
///    with explicit `bool` value as _`deprecated = bool`_.
///
/// * `description = "..."` Define possible description for the parameter as str.
///
/// * `style = ...` Defines how parameters are serialized by [`ParameterStyle`][style]. Default values are based on _`in`_ attribute.
///
/// * `explode` Defines whether new _`parameter=value`_ is created for each parameter withing _`object`_ or _`array`_.
///
/// * `allow_reserved` Defines whether reserved characters _`:/?#[]@!$&'()*+,;=`_ is allowed within value.
///
/// * `example = ...` Can method reference or _`json!(...)`_. Given example
///   will override any example in underlying parameter type.
///
/// ##### Parameter type attributes
///
/// These attributes supported when _`parameter_type`_ is present. Either by manually providing one
/// or otherwise resolved e.g from path macro argument when _`yaml`_ crate feature is
/// enabled.
///
/// * `format = ...` May either be variant of the [`KnownFormat`][known_format] enum, or otherwise
///   an open value as a string. By default the format is derived from the type of the property
///   according OpenApi spec.
///
/// * `write_only` Defines property is only used in **write** operations *POST,PUT,PATCH* but not in *GET*
///
/// * `read_only` Defines property is only used in **read** operations *GET* but not in *POST,PUT,PATCH*
///
/// * `xml(...)` Can be used to define [`Xml`][xml] object properties for the parameter type.
///    See configuration options at xml attributes of [`ToSchema`][to_schema_xml]
///
/// * `nullable` Defines property is nullable (note this is different to non-required).
///
/// * `multiple_of = ...` Can be used to define multiplier for a value. Value is considered valid
///   division will result an `integer`. Value must be strictly above _`0`_.
///
/// * `maximum = ...` Can be used to define inclusive upper bound to a `number` value.
///
/// * `minimum = ...` Can be used to define inclusive lower bound to a `number` value.
///
/// * `exclusive_maximum = ...` Can be used to define exclusive upper bound to a `number` value.
///
/// * `exclusive_minimum = ...` Can be used to define exclusive lower bound to a `number` value.
///
/// * `max_length = ...` Can be used to define maximum length for `string` types.
///
/// * `min_length = ...` Can be used to define minimum length for `string` types.
///
/// * `pattern = ...` Can be used to define valid regular expression in _ECMA-262_ dialect the field value must match.
///
/// * `max_items = ...` Can be used to define maximum items allowed for `array` fields. Value must
///   be non-negative integer.
///
/// * `min_items = ...` Can be used to define minimum items allowed for `array` fields. Value must
///   be non-negative integer.
///
/// **For example:**
///
/// ```text
/// params(
///     ("id" = String, Path, deprecated, description = "Pet database id"),
///     ("name", Path, deprecated, description = "Pet name"),
///     (
///         "value" = inline(Option<[String]>),
///         Query,
///         description = "Value description",
///         style = Form,
///         allow_reserved,
///         deprecated,
///         explode,
///         example = json!(["Value"])),
///         max_length = 10,
///         min_items = 1
///     )
/// )
/// ```
///
/// ## IntoParams Type
///
/// In the IntoParams parameters format, the parameters are specified using an identifier for a type
/// that implements [`IntoParams`][into_params]. See [`IntoParams`][into_params] for an
/// example.
///
/// ```text
/// params(MyParameters)
/// ```
///
/// **Note!** that `MyParameters` can also be used in combination with the [tuples
/// representation](#tuples) or other structs.
/// ```text
/// params(
///     MyParameters1,
///     MyParameters2,
///     ("id" = String, Path, deprecated, description = "Pet database id"),
/// )
/// ```
///
///
/// _**More minimal example with the defaults.**_
/// ```
/// # struct Pet {
/// #    id: u64,
/// #    name: String,
/// # }
/// #
/// #[salvo_oapi::path(
///    post,
///    path = "/pet",
///    request_body = Pet,
///    responses(
///         (status = 200, description = "Pet stored successfully", body = Pet,
///             headers(
///                 ("x-cache-len", description = "Cache length")
///             )
///         ),
///    ),
///    params(
///      ("x-csrf-token", Header, description = "Current csrf token of user"),
///    )
/// )]
/// fn post_pet(pet: Pet) -> Pet {
///     Pet {
///         id: 4,
///         name: "bob the cat".to_string(),
///     }
/// }
/// ```
///
/// _**Use of Rust's own `#[deprecated]` attribute will reflect to the generated OpenAPI spec and mark this operation as deprecated.**_
/// ```
/// # use serde_json::json;
/// #[salvo_oapi::path(
///     responses(
///         (status = 200, description = "Pet found from database")
///     ),
///     params(
///         ("id", description = "Pet id"),
///     )
/// )]
/// #[get("/pet/{id}")]
/// #[deprecated]
/// async fn get_pet_by_id(id: web::Path<i32>) -> impl Responder {
///     HttpResponse::Ok().json(json!({ "pet": format!("{:?}", &id.into_inner()) }))
/// }
/// ```
///
/// _**Define context path for endpoint. The resolved **path** shown in OpenAPI doc will be `/api/pet/{id}`.**_
/// ```
/// # use serde_json::json;
/// #[salvo_oapi::path(
///     context_path = "/api",
///     responses(
///         (status = 200, description = "Pet found from database")
///     )
/// )]
/// #[get("/pet/{id}")]
/// async fn get_pet_by_id(id: web::Path<i32>) -> impl Responder {
///     HttpResponse::Ok().json(json!({ "pet": format!("{:?}", &id.into_inner()) }))
/// }
/// ```
///
/// _**Example with multiple return types**_
/// ```
/// # trait User {}
/// # struct User1 {
/// #   id: String
/// # }
/// # impl User for User1 {}
/// #[salvo_oapi::path(
///     get,
///     path = "/user",
///     responses(
///         (status = 200, content(
///                 ("application/vnd.user.v1+json" = User1, example = json!({"id": "id".to_string()})),
///                 ("application/vnd.user.v2+json" = User2, example = json!({"id": 2}))
///             )
///         )
///     )
/// )]
/// fn get_user() -> Box<dyn User> {
///   Box::new(User1 {id: "id".to_string()})
/// }
/// ````
///
/// _**Example with multiple examples on single response.**_
///```rust
/// # #[derive(serde::Serialize, serde::Deserialize)]
/// # struct User {
/// #   name: String
/// # }
/// #[salvo_oapi::path(
///     get,
///     path = "/user",
///     responses(
///         (status = 200, body = User,
///             examples(
///                 ("Demo" = (summary = "This is summary", description = "Long description",
///                             value = json!(User{name: "Demo".to_string()}))),
///                 ("John" = (summary = "Another user", value = json!({"name": "John"})))
///              )
///         )
///     )
/// )]
/// fn get_user() -> User {
///   User {name: "John".to_string()}
/// }
///```
///
/// [in_enum]: salvo_oapi/openapi/path/enum.ParameterIn.html
/// [path]: trait.Path.html
/// [to_schema]: trait.ToSchema.html
/// [openapi]: derive.OpenApi.html
/// [security]: openapi/security/struct.SecurityRequirement.html
/// [security_schema]: openapi/security/struct.SecuritySchema.html
/// [primitive]: https://doc.rust-lang.org/std/primitive/index.html
/// [into_params]: trait.IntoParams.html
/// [style]: openapi/path/enum.ParameterStyle.html
/// [into_responses_trait]: trait.IntoResponses.html
/// [into_params_derive]: derive.IntoParams.html
/// [to_response_trait]: trait.ToResponse.html
/// [known_format]: openapi/schema/enum.KnownFormat.html
/// [xml]: openapi/xml/struct.Xml.html
/// [to_schema_xml]: macro@ToSchema#xml-attribute-configuration-options
pub fn endpoint(attr: TokenStream, item: TokenStream) -> TokenStream {
    let path_attribute = syn::parse_macro_input!(attr as PathAttr);

    let ast_fn = syn::parse::<ItemFn>(item).unwrap_or_abort();
    let fn_name = &*ast_fn.sig.ident.to_string();

    let path = Path::new(path_attribute, fn_name)
        .doc_comments(CommentAttributes::from_attributes(&ast_fn.attrs).0)
        .deprecated(ast_fn.attrs.iter().find_map(|attr| {
            if !matches!(attr.path().get_ident(), Some(ident) if &*ident.to_string() == "deprecated") {
                None
            } else {
                Some(true)
            }
        }));

    quote! {
       #path
        #ast_fn
    }
    .into()
}

#[proc_macro_error]
#[proc_macro_derive(IntoParams, attributes(param, into_params))]
/// Generate [path parameters][path_params] from struct's
/// fields.
///
/// This is `#[derive]` implementation for [`IntoParams`][into_params] trait.
///
/// Typically path parameters need to be defined within [`#[salvo_oapi::path(...params(...))]`][path_params] section
/// for the endpoint. But this trait eliminates the need for that when [`struct`][struct]s are used to define parameters.
/// Still [`std::primitive`] and [`String`](std::string::String) path parameters or [`tuple`] style path parameters need to be defined
/// within `params(...)` section if description or other than default configuration need to be given.
///
/// You can use the Rust's own `#[deprecated]` attribute on field to mark it as
/// deprecated and it will reflect to the generated OpenAPI spec.
///
/// `#[deprecated]` attribute supports adding additional details such as a reason and or since version
/// but this is is not supported in OpenAPI. OpenAPI has only a boolean flag to determine deprecation.
/// While it is totally okay to declare deprecated with reason
/// `#[deprecated  = "There is better way to do this"]` the reason would not render in OpenAPI spec.
///
/// Doc comment on struct fields will be used as description for the generated parameters.
/// ```
/// #[derive(salvo_oapi::IntoParams)]
/// struct Query {
///     /// Query todo items by name.
///     name: String
/// }
/// ```
///
/// # IntoParams Container Attributes for `#[into_params(...)]`
///
/// The following attributes are available for use in on the container attribute `#[into_params(...)]` for the struct
/// deriving `IntoParams`:
///
/// * `names(...)` Define comma separated list of names for unnamed fields of struct used as a path parameter.
///    __Only__ supported on __unnamed structs__.
/// * `style = ...` Defines how all parameters are serialized by [`ParameterStyle`][style]. Default
///    values are based on _`parameter_in`_ attribute.
/// * `parameter_in = ...` =  Defines where the parameters of this field are used with a value from
///    [`openapi::path::ParameterIn`][in_enum]. There is no default value, if this attribute is not
///    supplied, then the value is determined by the `parameter_in_provider` in
///    [`IntoParams::into_params()`](trait.IntoParams.html#tymethod.into_params).
/// * `rename_all = ...` Can be provided to alternatively to the serde's `rename_all` attribute. Effectively provides same functionality.
///
/// Use `names` to define name for single unnamed argument.
/// ```
/// # use salvo_oapi::IntoParams;
/// #
/// #[derive(IntoParams)]
/// #[into_params(names("id"))]
/// struct Id(u64);
/// ```
///
/// Use `names` to define names for multiple unnamed arguments.
/// ```
/// # use salvo_oapi::IntoParams;
/// #
/// #[derive(IntoParams)]
/// #[into_params(names("id", "name"))]
/// struct IdAndName(u64, String);
/// ```
///
/// # IntoParams Field Attributes for `#[param(...)]`
///
/// The following attributes are available for use in the `#[param(...)]` on struct fields:
///
/// * `style = ...` Defines how the parameter is serialized by [`ParameterStyle`][style]. Default values are based on _`parameter_in`_ attribute.
///
/// * `explode` Defines whether new _`parameter=value`_ pair is created for each parameter withing _`object`_ or _`array`_.
///
/// * `allow_reserved` Defines whether reserved characters _`:/?#[]@!$&'()*+,;=`_ is allowed within value.
///
/// * `example = ...` Can be method reference or _`json!(...)`_. Given example
///   will override any example in underlying parameter type.
///
/// * `value_type = ...` Can be used to override default type derived from type of the field used in OpenAPI spec.
///   This is useful in cases where the default type does not correspond to the actual type e.g. when
///   any third-party types are used which are not [`ToSchema`][to_schema]s nor [`primitive` types][primitive].
///    Value can be any Rust type what normally could be used to serialize to JSON or custom type such as _`Object`_.
///    _`Object`_ will be rendered as generic OpenAPI object.
///
/// * `inline` If set, the schema for this field's type needs to be a [`ToSchema`][to_schema], and
///   the schema definition will be inlined.
///
/// * `default = ...` Can be method reference or _`json!(...)`_.
///
/// * `format = ...` May either be variant of the [`KnownFormat`][known_format] enum, or otherwise
///   an open value as a string. By default the format is derived from the type of the property
///   according OpenApi spec.
///
/// * `write_only` Defines property is only used in **write** operations *POST,PUT,PATCH* but not in *GET*
///
/// * `read_only` Defines property is only used in **read** operations *GET* but not in *POST,PUT,PATCH*
///
/// * `xml(...)` Can be used to define [`Xml`][xml] object properties applicable to named fields.
///    See configuration options at xml attributes of [`ToSchema`][to_schema_xml]
///
/// * `nullable` Defines property is nullable (note this is different to non-required).
///
/// * `required = ...` Can be used to enforce required status for the parameter. [See
///    rules][derive@IntoParams#field-nullability-and-required-rules]
///
/// * `rename = ...` Can be provided to alternatively to the serde's `rename` attribute. Effectively provides same functionality.
///
/// * `multiple_of = ...` Can be used to define multiplier for a value. Value is considered valid
///   division will result an `integer`. Value must be strictly above _`0`_.
///
/// * `maximum = ...` Can be used to define inclusive upper bound to a `number` value.
///
/// * `minimum = ...` Can be used to define inclusive lower bound to a `number` value.
///
/// * `exclusive_maximum = ...` Can be used to define exclusive upper bound to a `number` value.
///
/// * `exclusive_minimum = ...` Can be used to define exclusive lower bound to a `number` value.
///
/// * `max_length = ...` Can be used to define maximum length for `string` types.
///
/// * `min_length = ...` Can be used to define minimum length for `string` types.
///
/// * `pattern = ...` Can be used to define valid regular expression in _ECMA-262_ dialect the field value must match.
///
/// * `max_items = ...` Can be used to define maximum items allowed for `array` fields. Value must
///   be non-negative integer.
///
/// * `min_items = ...` Can be used to define minimum items allowed for `array` fields. Value must
///   be non-negative integer.
///
/// * `with_schema = ...` Use _`schema`_ created by provided function reference instead of the
///   default derived _`schema`_. The function must match to `fn() -> Into<RefOr<Schema>>`. It does
///   not accept arguments and must return anything that can be converted into `RefOr<Schema>`.
///
/// * `additional_properties = ...` Can be used to define free form types for maps such as
///   [`HashMap`](std::collections::HashMap) and [`BTreeMap`](std::collections::BTreeMap).
///   Free form type enables use of arbitrary types within map values.
///   Supports formats _`additional_properties`_ and _`additional_properties = true`_.
///
/// #### Field nullability and required rules
///
/// Same rules for nullability and required status apply for _`IntoParams`_ field attributes as for
/// _`ToSchema`_ field attributes. [See the rules][`derive@ToSchema#field-nullability-and-required-rules`].
///
/// # Partial `#[serde(...)]` attributes support
///
/// IntoParams derive has partial support for [serde attributes]. These supported attributes will reflect to the
/// generated OpenAPI doc. The following attributes are currently supported:
///
/// * `rename_all = "..."` Supported at the container level.
/// * `rename = "..."` Supported **only** at the field level.
/// * `default` Supported at the container level and field level according to [serde attributes].
/// * `skip_serializing_if = "..."` Supported  **only** at the field level.
/// * `with = ...` Supported **only** at field level.
///
/// Other _`serde`_ attributes will impact the serialization but will not be reflected on the generated OpenAPI doc.
///
/// # Examples
///
/// _**Demonstrate [`IntoParams`][into_params] usage with resolving `Path` and `Query` parameters
/// with _`salvo`_**_.
/// ```
/// use serde::Deserialize;
/// use serde_json::json;
/// use salvo_oapi::IntoParams;
///
/// #[derive(Deserialize, IntoParams)]
/// struct PetPathArgs {
///     /// Id of pet
///     id: i64,
///     /// Name of pet
///     name: String,
/// }
///
/// #[derive(Deserialize, IntoParams)]
/// struct Filter {
///     /// Age filter for pets
///     #[deprecated]
///     #[param(style = Form, explode, allow_reserved, example = json!([10]))]
///     age: Option<Vec<i32>>,
/// }
///
/// #[salvo_oapi::path(
///     params(PetPathArgs, Filter),
///     responses(
///         (status = 200, description = "success response")
///     )
/// )]
/// #[get("/pet/{id}/{name}")]
/// async fn get_pet(pet: Path<PetPathArgs>, query: Query<Filter>) -> impl Responder {
///     HttpResponse::Ok().json(json!({ "id": pet.id }))
/// }
/// ```
///
/// _**Demonstrate [`IntoParams`][into_params] usage with the `#[into_params(...)]` container attribute to
/// be used as a path query, and inlining a schema query field:**_
/// ```
/// use serde::Deserialize;
/// use salvo_oapi::{IntoParams, ToSchema};
///
/// #[derive(Deserialize, ToSchema)]
/// #[serde(rename_all = "snake_case")]
/// enum PetKind {
///     Dog,
///     Cat,
/// }
///
/// #[derive(Deserialize, IntoParams)]
/// #[into_params(style = Form, parameter_in = Query)]
/// struct PetQuery {
///     /// Name of pet
///     name: Option<String>,
///     /// Age of pet
///     age: Option<i32>,
///     /// Kind of pet
///     #[param(inline)]
///     kind: PetKind
/// }
///
/// #[salvo_oapi::path(
///     get,
///     path = "/get_pet",
///     params(PetQuery),
///     responses(
///         (status = 200, description = "success response")
///     )
/// )]
/// async fn get_pet(query: PetQuery) {
///     // ...
/// }
/// ```
///
/// _**Override `String` with `i64` using `value_type` attribute.**_
/// ```
/// # use salvo_oapi::IntoParams;
/// #
/// #[derive(IntoParams)]
/// #[into_params(parameter_in = Query)]
/// struct Filter {
///     #[param(value_type = i64)]
///     id: String,
/// }
/// ```
///
/// _**Override `String` with `Object` using `value_type` attribute. _`Object`_ will render as `type: object` in OpenAPI spec.**_
/// ```
/// # use salvo_oapi::IntoParams;
/// #
/// #[derive(IntoParams)]
/// #[into_params(parameter_in = Query)]
/// struct Filter {
///     #[param(value_type = Object)]
///     id: String,
/// }
/// ```
///
/// _**You can use a generic type to override the default type of the field.**_
/// ```
/// # use salvo_oapi::IntoParams;
/// #
/// #[derive(IntoParams)]
/// #[into_params(parameter_in = Query)]
/// struct Filter {
///     #[param(value_type = Option<String>)]
///     id: String
/// }
/// ```
///
/// _**You can even override a [`Vec`] with another one.**_
/// ```
/// # use salvo_oapi::IntoParams;
/// #
/// #[derive(IntoParams)]
/// #[into_params(parameter_in = Query)]
/// struct Filter {
///     #[param(value_type = Vec<i32>)]
///     id: Vec<String>
/// }
/// ```
///
/// _**We can override value with another [`ToSchema`][to_schema].**_
/// ```
/// # use salvo_oapi::{IntoParams, ToSchema};
/// #
/// #[derive(ToSchema)]
/// struct Id {
///     value: i64,
/// }
///
/// #[derive(IntoParams)]
/// #[into_params(parameter_in = Query)]
/// struct Filter {
///     #[param(value_type = Id)]
///     id: String
/// }
/// ```
///
/// _**Example with validation attributes.**_
/// ```
/// #[derive(salvo_oapi::IntoParams)]
/// struct Item {
///     #[param(maximum = 10, minimum = 5, multiple_of = 2.5)]
///     id: i32,
///     #[param(max_length = 10, min_length = 5, pattern = "[a-z]*")]
///     value: String,
///     #[param(max_items = 5, min_items = 1)]
///     items: Vec<String>,
/// }
/// ````
///
/// _**Use `schema_with` to manually implement schema for a field.**_
/// ```
/// # use salvo_oapi::openapi::schema::{Object, ObjectBuilder};
/// fn custom_type() -> Object {
///     ObjectBuilder::new()
///         .schema_type(salvo_oapi::openapi::SchemaType::String)
///         .format(Some(salvo_oapi::openapi::SchemaFormat::Custom(
///             "email".to_string(),
///         )))
///         .description(Some("this is the description"))
///         .build()
/// }
///
/// #[derive(salvo_oapi::IntoParams)]
/// #[into_params(parameter_in = Query)]
/// struct Query {
///     #[param(schema_with = custom_type)]
///     email: String,
/// }
/// ```
///
/// [to_schema]: trait.ToSchema.html
/// [known_format]: openapi/schema/enum.KnownFormat.html
/// [xml]: openapi/xml/struct.Xml.html
/// [into_params]: trait.IntoParams.html
/// [path_params]: attr.path.html#params-attributes
/// [struct]: https://doc.rust-lang.org/std/keyword.struct.html
/// [style]: openapi/path/enum.ParameterStyle.html
/// [in_enum]: salvo_oapi/openapi/path/enum.ParameterIn.html
/// [primitive]: https://doc.rust-lang.org/std/primitive/index.html
/// [serde attributes]: https://serde.rs/attributes.html
/// [to_schema_xml]: macro@ToSchema#xml-attribute-configuration-options
pub fn into_params(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = syn::parse_macro_input!(input);

    let into_params = IntoParams {
        attrs,
        generics,
        data,
        ident,
    };

    into_params.to_token_stream().into()
}

#[proc_macro_error]
#[proc_macro_derive(ToResponse, attributes(response, content, to_schema))]
/// Generate reusable OpenAPI response what can be used
/// in [`salvo_oapi::path`][path] or in [`OpenApi`][openapi].
///
/// This is `#[derive]` implementation for [`ToResponse`][to_response] trait.
///
///
/// _`#[response]`_ attribute can be used to alter and add [response attributes](#toresponse-response-attributes).
///
/// _`#[content]`_ attributes is used to make enum variant a content of a specific type for the
/// response.
///
/// _`#[to_schema]`_ attribute is used to inline a schema for a response in unnamed structs or
/// enum variants with `#[content]` attribute. **Note!** [`ToSchema`] need to be implemented for
/// the field or variant type.
///
/// Type derived with _`ToResponse`_ uses provided doc comment as a description for the response. It
/// can alternatively be overridden with _`description = ...`_ attribute.
///
/// _`ToResponse`_ can be used in four different ways to generate OpenAPI response component.
///
/// 1. By decorating `struct` or `enum` with [`derive@ToResponse`] derive macro. This will create a
///    response with inlined schema resolved from the fields of the `struct` or `variants` of the
///    enum.
///
///    ```rust
///     # use salvo_oapi::ToResponse;
///     #[derive(ToResponse)]
///     #[response(description = "Person response returns single Person entity")]
///     struct Person {
///         name: String,
///     }
///    ```
///
/// 2. By decorating unnamed field `struct` with [`derive@ToResponse`] derive macro. Unnamed field struct
///    allows users to use new type pattern to define one inner field which is used as a schema for
///    the generated response. This allows users to define `Vec` and `Option` response types.
///    Additionally these types can also be used with `#[to_schema]` attribute to inline the
///    field's type schema if it implements [`ToSchema`] derive macro.
///
///    ```rust
///     # #[derive(salvo_oapi::ToSchema)]
///     # struct Person {
///     #     name: String,
///     # }
///     /// Person list response
///     #[derive(salvo_oapi::ToResponse)]
///     struct PersonList(Vec<Person>);
///    ```
///
/// 3. By decorating unit struct with [`derive@ToResponse`] derive macro. Unit structs will produce a
///    response without body.
///
///    ```rust
///     /// Success response which does not have body.
///     #[derive(salvo_oapi::ToResponse)]
///     struct SuccessResponse;
///    ```
///
/// 4. By decorating `enum` with variants having `#[content(...)]` attribute. This allows users to
///    define multiple response content schemas to single response according to OpenAPI spec.
///    **Note!** Enum with _`content`_ attribute in variants cannot have enum level _`example`_ or
///    _`examples`_ defined. Instead examples need to be defined per variant basis. Additionally
///    these variants can also be used with `#[to_schema]` attribute to inline the variant's type schema
///    if it implements [`ToSchema`] derive macro.
///
///    ```rust
///     #[derive(salvo_oapi::ToSchema)]
///     struct Admin {
///         name: String,
///     }
///     #[derive(salvo_oapi::ToSchema)]
///     struct Admin2 {
///         name: String,
///         id: i32,
///     }
///
///     #[derive(salvo_oapi::ToResponse)]
///     enum Person {
///         #[response(examples(
///             ("Person1" = (value = json!({"name": "name1"}))),
///             ("Person2" = (value = json!({"name": "name2"})))
///         ))]
///         Admin(#[content("application/vnd-custom-v1+json")] Admin),
///
///         #[response(example = json!({"name": "name3", "id": 1}))]
///         Admin2(#[content("application/vnd-custom-v2+json")] #[to_schema] Admin2),
///     }
///    ```
///
/// # ToResponse `#[response(...)]` attributes
///
/// * `description = "..."` Define description for the response as str. This can be used to
///   override the default description resolved from doc comments if present.
///
/// * `content_type = "..." | content_type = [...]` Can be used to override the default behavior of auto resolving the content type
///   from the `body` attribute. If defined the value should be valid content type such as
///   _`application/json`_. By default the content type is _`text/plain`_ for
///   [primitive Rust types][primitive], `application/octet-stream` for _`[u8]`_ and
///   _`application/json`_ for struct and complex enum types.
///   Content type can also be slice of **content_type** values if the endpoint support returning multiple
///  response content types. E.g _`["application/json", "text/xml"]`_ would indicate that endpoint can return both
///  _`json`_ and _`xml`_ formats. **The order** of the content types define the default example show first in
///  the Swagger UI. Swagger UI wil use the first _`content_type`_ value as a default example.
///
/// * `headers(...)` Slice of response headers that are returned back to a caller.
///
/// * `example = ...` Can be _`json!(...)`_. _`json!(...)`_ should be something that
///   _`serde_json::json!`_ can parse as a _`serde_json::Value`_.
///
/// * `examples(...)` Define multiple examples for single response. This attribute is mutually
///   exclusive to the _`example`_ attribute and if both are defined this will override the _`example`_.
///     * `name = ...` This is first attribute and value must be literal string.
///     * `summary = ...` Short description of example. Value must be literal string.
///     * `description = ...` Long description of example. Attribute supports markdown for rich text
///       representation. Value must be literal string.
///     * `value = ...` Example value. It must be _`json!(...)`_. _`json!(...)`_ should be something that
///       _`serde_json::json!`_ can parse as a _`serde_json::Value`_.
///     * `external_value = ...` Define URI to literal example value. This is mutually exclusive to
///       the _`value`_ attribute. Value must be literal string.
///
///      _**Example of example definition.**_
///     ```text
///      ("John" = (summary = "This is John", value = json!({"name": "John"})))
///     ```
///
/// # Examples
///
/// _**Use reusable response in operation handler.**_
/// ```
/// #[derive(salvo_oapi::ToResponse)]
/// struct PersonResponse {
///    value: String
/// }
///
/// #[derive(salvo_oapi::OpenApi)]
/// #[openapi(components(responses(PersonResponse)))]
/// struct Doc;
///
/// #[salvo_oapi::path(
///     get,
///     path = "/api/person",
///     responses(
///         (status = 200, response = PersonResponse)
///     )
/// )]
/// fn get_person() -> PersonResponse {
///     PersonResponse { value: "person".to_string() }
/// }
/// ```
///
/// _**Create a response from named struct.**_
/// ```
///  /// This is description
///  ///
///  /// It will also be used in `ToSchema` if present
///  #[derive(salvo_oapi::ToSchema, salvo_oapi::ToResponse)]
///  #[response(
///      description = "Override description for response",
///      content_type = "text/xml"
///  )]
///  #[response(
///      example = json!({"name": "the name"}),
///      headers(
///          ("csrf-token", description = "response csrf token"),
///          ("random-id" = i32)
///      )
///  )]
///  struct Person {
///      name: String,
///  }
/// ```
///
/// _**Create inlined person list response.**_
/// ```
///  # #[derive(salvo_oapi::ToSchema)]
///  # struct Person {
///  #     name: String,
///  # }
///  /// Person list response
///  #[derive(salvo_oapi::ToResponse)]
///  struct PersonList(#[to_schema] Vec<Person>);
/// ```
///
/// _**Create enum response from variants.**_
/// ```
///  #[derive(salvo_oapi::ToResponse)]
///  enum PersonType {
///      Value(String),
///      Foobar,
///  }
/// ```
///
/// [to_response]: trait.ToResponse.html
/// [primitive]: https://doc.rust-lang.org/std/primitive/index.html
/// [path]: attr.path.html
/// [openapi]: derive.OpenApi.html
pub fn to_response(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = syn::parse_macro_input!(input);

    let response = ToResponse::new(attrs, &data, generics, ident);

    response.to_token_stream().into()
}

#[proc_macro_error]
#[proc_macro_derive(IntoResponses, attributes(response, to_schema, ref_response, to_response))]
/// Generate responses with status codes what
/// can be attached to the [`salvo_oapi::path`][path_into_responses].
///
/// This is `#[derive]` implementation of [`IntoResponses`][into_responses] trait. [`derive@IntoResponses`]
/// can be used to decorate _`structs`_ and _`enums`_ to generate response maps that can be used in
/// [`salvo_oapi::path`][path_into_responses]. If _`struct`_ is decorated with [`derive@IntoResponses`] it will be
/// used to create a map of responses containing single response. Decorating _`enum`_ with
/// [`derive@IntoResponses`] will create a map of responses with a response for each variant of the _`enum`_.
///
/// Named field _`struct`_ decorated with [`derive@IntoResponses`] will create a response with inlined schema
/// generated from the body of the struct. This is a conveniency which allows users to directly
/// create responses with schemas without first creating a separate [response][to_response] type.
///
/// Unit _`struct`_ behaves similarly to then named field struct. Only difference is that it will create
/// a response without content since there is no inner fields.
///
/// Unnamed field _`struct`_ decorated with [`derive@IntoResponses`] will by default create a response with
/// referenced [schema][to_schema] if field is object or schema if type is [primitive
/// type][primitive]. _`#[to_schema]`_ attribute at field of unnamed _`struct`_ can be used to inline
/// the schema if type of the field implements [`ToSchema`][to_schema] trait. Alternatively
/// _`#[to_response]`_ and _`#[ref_response]`_ can be used at field to either reference a reusable
/// [response][to_response] or inline a reusable [response][to_response]. In both cases the field
/// type is expected to implement [`ToResponse`][to_response] trait.
///
///
/// Enum decorated with [`derive@IntoResponses`] will create a response for each variant of the _`enum`_.
/// Each variant must have it's own _`#[response(...)]`_ definition. Unit variant will behave same
/// as unit _`struct`_ by creating a response without content. Similarly named field variant and
/// unnamed field variant behaves the same as it was named field _`struct`_ and unnamed field
/// _`struct`_.
///
/// _`#[response]`_ attribute can be used at named structs, unnamed structs, unit structs and enum
/// variants to alter [response attributes](#intoresponses-response-attributes) of responses.
///
/// Doc comment on a _`struct`_ or _`enum`_ variant will be used as a description for the response.
/// It can also be overridden with _`description = "..."`_ attribute.
///
/// # IntoResponses `#[response(...)]` attributes
///
/// * `status = ...` Must be provided. Is either a valid http status code integer. E.g. _`200`_ or a
///   string value representing a range such as _`"4XX"`_ or `"default"` or a valid _`http::status::StatusCode`_.
///   _`StatusCode`_ can either be use path to the status code or _status code_ constant directly.
///
/// * `description = "..."` Define description for the response as str. This can be used to
///   override the default description resolved from doc comments if present.
///
/// * `content_type = "..." | content_type = [...]` Can be used to override the default behavior of auto resolving the content type
///   from the `body` attribute. If defined the value should be valid content type such as
///   _`application/json`_. By default the content type is _`text/plain`_ for
///   [primitive Rust types][primitive], `application/octet-stream` for _`[u8]`_ and
///   _`application/json`_ for struct and complex enum types.
///   Content type can also be slice of **content_type** values if the endpoint support returning multiple
///  response content types. E.g _`["application/json", "text/xml"]`_ would indicate that endpoint can return both
///  _`json`_ and _`xml`_ formats. **The order** of the content types define the default example show first in
///  the Swagger UI. Swagger UI wil use the first _`content_type`_ value as a default example.
///
/// * `headers(...)` Slice of response headers that are returned back to a caller.
///
/// * `example = ...` Can be _`json!(...)`_. _`json!(...)`_ should be something that
///   _`serde_json::json!`_ can parse as a _`serde_json::Value`_.
///
/// * `examples(...)` Define multiple examples for single response. This attribute is mutually
///   exclusive to the _`example`_ attribute and if both are defined this will override the _`example`_.
///     * `name = ...` This is first attribute and value must be literal string.
///     * `summary = ...` Short description of example. Value must be literal string.
///     * `description = ...` Long description of example. Attribute supports markdown for rich text
///       representation. Value must be literal string.
///     * `value = ...` Example value. It must be _`json!(...)`_. _`json!(...)`_ should be something that
///       _`serde_json::json!`_ can parse as a _`serde_json::Value`_.
///     * `external_value = ...` Define URI to literal example value. This is mutually exclusive to
///       the _`value`_ attribute. Value must be literal string.
///
///      _**Example of example definition.**_
///     ```text
///      ("John" = (summary = "This is John", value = json!({"name": "John"})))
///     ```
///
/// # Examples
///
/// _**Use `IntoResponses` to define [`salvo_oapi::path`][path] responses.**_
/// ```
/// #[derive(salvo_oapi::ToSchema)]
/// struct BadRequest {
///     message: String,
/// }
///
/// #[derive(salvo_oapi::IntoResponses)]
/// enum UserResponses {
///     /// Success response
///     #[response(status = 200)]
///     Success { value: String },
///
///     #[response(status = 404)]
///     NotFound,
///
///     #[response(status = 400)]
///     BadRequest(BadRequest),
/// }
///
/// #[salvo_oapi::path(
///     get,
///     path = "/api/user",
///     responses(
///         UserResponses
///     )
/// )]
/// fn get_user() -> UserResponses {
///    UserResponses::NotFound
/// }
/// ```
/// _**Named struct response with inlined schema.**_
/// ```
/// /// This is success response
/// #[derive(salvo_oapi::IntoResponses)]
/// #[response(status = 200)]
/// struct SuccessResponse {
///     value: String,
/// }
/// ```
///
/// _**Unit struct response without content.**_
/// ```
/// #[derive(salvo_oapi::IntoResponses)]
/// #[response(status = NOT_FOUND)]
/// struct NotFound;
/// ```
///
/// _**Unnamed struct response with inlined response schema.**_
/// ```
/// # #[derive(salvo_oapi::ToSchema)]
/// # struct Foo;
/// #[derive(salvo_oapi::IntoResponses)]
/// #[response(status = 201)]
/// struct CreatedResponse(#[to_schema] Foo);
/// ```
///
/// _**Enum with multiple responses.**_
/// ```
/// # #[derive(salvo_oapi::ToResponse)]
/// # struct Response {
/// #     message: String,
/// # }
/// # #[derive(salvo_oapi::ToSchema)]
/// # struct BadRequest {}
/// #[derive(salvo_oapi::IntoResponses)]
/// enum UserResponses {
///     /// Success response description.
///     #[response(status = 200)]
///     Success { value: String },
///
///     #[response(status = 404)]
///     NotFound,
///
///     #[response(status = 400)]
///     BadRequest(BadRequest),
///
///     #[response(status = 500)]
///     ServerError(#[ref_response] Response),
///
///     #[response(status = 418)]
///     TeaPot(#[to_response] Response),
/// }
/// ```
///
/// [into_responses]: trait.IntoResponses.html
/// [to_schema]: trait.ToSchema.html
/// [to_response]: trait.ToResponse.html
/// [path_into_responses]: attr.path.html#responses-from-intoresponses
/// [primitive]: https://doc.rust-lang.org/std/primitive/index.html
/// [path]: macro@crate::path
pub fn into_responses(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = syn::parse_macro_input!(input);

    let into_responses = IntoResponses {
        attributes: attrs,
        ident,
        generics,
        data,
    };

    into_responses.to_token_stream().into()
}

/// Create OpenAPI Schema from arbitrary type.
///
/// This macro provides a quick way to render arbitrary types as OpenAPI Schema Objects. It
/// supports two call formats.
/// 1. With type only
/// 2. With _`#[inline]`_ attribute to inline the referenced schemas.
///
/// By default the macro will create references `($ref)` for non primitive types like _`Pet`_.
/// However when used with _`#[inline]`_ the non [`primitive`][primitive] type schemas will
/// be inlined to the schema output.
///
/// ```
/// # #[derive(salvo_oapi::ToSchema)]
/// # struct Pet {id: i32};
/// let schema = salvo_oapi::schema!(Vec<Pet>);
///
/// // with inline
/// let schema = salvo_oapi::schema!(#[inline] Vec<Pet>);
/// ```
///
/// # Examples
///
/// _**Create vec of pets schema.**_
/// ```
/// # use salvo_oapi::openapi::schema::{Schema, Array, Object, ObjectBuilder, SchemaFormat,
/// # KnownFormat, SchemaType};
/// # use salvo_oapi::openapi::RefOr;
/// #[derive(salvo_oapi::ToSchema)]
/// struct Pet {
///     id: i32,
///     name: String,
/// }
///
/// let schema: RefOr<Schema> = salvo_oapi::schema!(#[inline] Vec<Pet>).into();
/// // will output
/// let generated = RefOr::T(Schema::Array(
///     Array::new(
///         ObjectBuilder::new()
///             .property("id", ObjectBuilder::new()
///                 .schema_type(SchemaType::Integer)
///                 .format(Some(SchemaFormat::KnownFormat(KnownFormat::Int32)))
///                 .build())
///             .required("id")
///             .property("name", Object::with_type(SchemaType::String))
///             .required("name")
///     )
/// ));
/// # assert_json_diff::assert_json_eq!(serde_json::to_value(&schema).unwrap(), serde_json::to_value(&generated).unwrap());
/// ```
///
/// [primitive]: https://doc.rust-lang.org/std/primitive/index.html
#[proc_macro]
pub fn schema(input: TokenStream) -> TokenStream {
    struct Schema {
        inline: bool,
        ty: syn::Type,
    }
    impl Parse for Schema {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            let inline = if input.peek(Token![#]) && input.peek2(Bracket) {
                input.parse::<Token![#]>()?;

                let inline;
                bracketed!(inline in input);
                let i = inline.parse::<Ident>()?;
                i == "inline"
            } else {
                false
            };

            let ty = input.parse()?;

            Ok(Self { inline, ty })
        }
    }

    let schema = syn::parse_macro_input!(input as Schema);
    let type_tree = TypeTree::from_type(&schema.ty);

    let schema = ComponentSchema::new(ComponentSchemaProps {
        features: Some(vec![Feature::Inline(schema.inline.into())]),
        type_tree: &type_tree,
        deprecated: None,
        description: None,
        object_name: "",
    });
    schema.to_token_stream().into()
    // let stream = schema.to_token_stream().into();
    // println!("{}", stream);
    // stream
}

/// Tokenizes slice or Vec of tokenizable items as array either with reference (`&[...]`)
/// or without correctly to OpenAPI JSON.
#[derive(Debug)]
enum Array<'a, T>
where
    T: Sized + ToTokens,
{
    Owned(Vec<T>),
    #[allow(dead_code)]
    Borrowed(&'a [T]),
}

impl<T> Array<'_, T> where T: ToTokens + Sized {}

impl<V> FromIterator<V> for Array<'_, V>
where
    V: Sized + ToTokens,
{
    fn from_iter<T: IntoIterator<Item = V>>(iter: T) -> Self {
        Self::Owned(iter.into_iter().collect())
    }
}

impl<'a, T> Deref for Array<'a, T>
where
    T: Sized + ToTokens,
{
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(vec) => vec.as_slice(),
            Self::Borrowed(slice) => slice,
        }
    }
}

impl<T> ToTokens for Array<'_, T>
where
    T: Sized + ToTokens,
{
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let values = match self {
            Self::Owned(values) => values.iter(),
            Self::Borrowed(values) => values.iter(),
        };

        tokens.append(Group::new(
            proc_macro2::Delimiter::Bracket,
            values
                .fold(Punctuated::new(), |mut punctuated, item| {
                    punctuated.push_value(item);
                    punctuated.push_punct(Punct::new(',', proc_macro2::Spacing::Alone));

                    punctuated
                })
                .to_token_stream(),
        ));
    }
}

#[derive(Debug)]
enum Deprecated {
    True,
    False,
}

impl From<bool> for Deprecated {
    fn from(bool: bool) -> Self {
        if bool {
            Self::True
        } else {
            Self::False
        }
    }
}

impl ToTokens for Deprecated {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let root = crate::root_crate();
        tokens.extend(match self {
            Self::False => quote! { #root::oapi::openapi::Deprecated::False },
            Self::True => quote! { #root::oapi::openapi::Deprecated::True },
        })
    }
}

#[derive(PartialEq, Eq, Debug)]
enum Required {
    True,
    False,
}

impl From<bool> for Required {
    fn from(bool: bool) -> Self {
        if bool {
            Self::True
        } else {
            Self::False
        }
    }
}

impl From<features::Required> for Required {
    fn from(value: features::Required) -> Self {
        let features::Required(required) = value;
        crate::Required::from(required)
    }
}

impl ToTokens for Required {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let root = crate::root_crate();
        tokens.extend(match self {
            Self::False => quote! { #root::oapi::openapi::Required::False },
            Self::True => quote! { #root::oapi::openapi::Required::True },
        })
    }
}

#[derive(Default, Debug)]
struct ExternalDocs {
    url: String,
    description: Option<String>,
}

impl Parse for ExternalDocs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        const EXPECTED_ATTRIBUTE: &str = "unexpected attribute, expected any of: url, description";

        let mut external_docs = ExternalDocs::default();

        while !input.is_empty() {
            let ident = input
                .parse::<Ident>()
                .map_err(|error| syn::Error::new(error.span(), format!("{EXPECTED_ATTRIBUTE}, {error}")))?;
            let attribute_name = &*ident.to_string();

            match attribute_name {
                "url" => {
                    external_docs.url = parse_utils::parse_next_literal_str(input)?;
                }
                "description" => {
                    external_docs.description = Some(parse_utils::parse_next_literal_str(input)?);
                }
                _ => return Err(syn::Error::new(ident.span(), EXPECTED_ATTRIBUTE)),
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(external_docs)
    }
}

impl ToTokens for ExternalDocs {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let root = crate::root_crate();
        let url = &self.url;
        tokens.extend(quote! {
            #root::oapi::openapi::external_docs::ExternalDocsBuilder::new()
                .url(#url)
        });

        if let Some(ref description) = self.description {
            tokens.extend(quote! {
                .description(Some(#description))
            });
        }

        tokens.extend(quote! { .build() })
    }
}

/// Represents OpenAPI Any value used in example and default fields.
#[derive(Clone, Debug)]
pub(self) enum AnyValue {
    String(TokenStream2),
    Json(TokenStream2),
    DefaultTrait { struct_ident: Ident, field_ident: Member },
}

impl AnyValue {
    /// Parse `json!(...)` as [`AnyValue::Json`]
    fn parse_json(input: ParseStream) -> syn::Result<Self> {
        parse_utils::parse_json_token_stream(input).map(AnyValue::Json)
    }

    fn parse_any(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Lit) {
            if input.peek(LitStr) {
                let lit_str = input.parse::<LitStr>().unwrap().to_token_stream();

                Ok(AnyValue::Json(lit_str))
            } else {
                let lit = input.parse::<Lit>().unwrap().to_token_stream();

                Ok(AnyValue::Json(lit))
            }
        } else {
            let fork = input.fork();
            let is_json = if fork.peek(syn::Ident) && fork.peek2(Token![!]) {
                let ident = fork.parse::<Ident>().unwrap();
                ident == "json"
            } else {
                false
            };

            if is_json {
                let json = parse_utils::parse_json_token_stream(input)?;

                Ok(AnyValue::Json(json))
            } else {
                let method = input.parse::<ExprPath>().map_err(|error| {
                    syn::Error::new(error.span(), "expected literal value, json!(...) or method reference")
                })?;

                Ok(AnyValue::Json(quote! { #method() }))
            }
        }
    }

    fn parse_lit_str_or_json(input: ParseStream) -> syn::Result<Self> {
        if input.peek(LitStr) {
            Ok(AnyValue::String(input.parse::<LitStr>().unwrap().to_token_stream()))
        } else {
            Ok(AnyValue::Json(parse_utils::parse_json_token_stream(input)?))
        }
    }

    fn new_default_trait(struct_ident: Ident, field_ident: Member) -> Self {
        Self::DefaultTrait {
            struct_ident,
            field_ident,
        }
    }
}

impl ToTokens for AnyValue {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        match self {
            Self::Json(json) => tokens.extend(quote! {
                serde_json::json!(#json)
            }),
            Self::String(string) => string.to_tokens(tokens),
            Self::DefaultTrait {
                struct_ident,
                field_ident,
            } => tokens.extend(quote! {
                serde_json::to_value(#struct_ident::default().#field_ident).unwrap()
            }),
        }
    }
}

trait ResultExt<T> {
    fn unwrap_or_abort(self) -> T;
    fn expect_or_abort(self, message: &str) -> T;
}

impl<T> ResultExt<T> for Result<T, syn::Error> {
    fn unwrap_or_abort(self) -> T {
        match self {
            Ok(value) => value,
            Err(error) => abort!(error.span(), format!("{error}")),
        }
    }

    fn expect_or_abort(self, message: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => abort!(error.span(), format!("{error}: {message}")),
        }
    }
}

trait OptionExt<T> {
    fn expect_or_abort(self, message: &str) -> T;
}

impl<T> OptionExt<T> for Option<T> {
    fn expect_or_abort(self, message: &str) -> T {
        self.unwrap_or_else(|| abort!(Span::call_site(), message))
    }
}

/// Parsing utils
mod parse_utils {
    use proc_macro2::{Group, Ident, TokenStream};
    use syn::{
        parenthesized,
        parse::{Parse, ParseStream},
        punctuated::Punctuated,
        token::Comma,
        Error, LitBool, LitStr, Token,
    };

    use crate::ResultExt;

    pub fn parse_next<T: Sized>(input: ParseStream, next: impl FnOnce() -> T) -> T {
        input
            .parse::<Token![=]>()
            .expect_or_abort("expected equals token before value assignment");
        next()
    }

    pub fn parse_next_literal_str(input: ParseStream) -> syn::Result<String> {
        Ok(parse_next(input, || input.parse::<LitStr>())?.value())
    }

    pub fn parse_groups<T, R>(input: ParseStream) -> syn::Result<R>
    where
        T: Sized,
        T: Parse,
        R: FromIterator<T>,
    {
        Punctuated::<Group, Comma>::parse_terminated(input).and_then(|groups| {
            groups
                .into_iter()
                .map(|group| syn::parse2::<T>(group.stream()))
                .collect::<syn::Result<R>>()
        })
    }

    pub fn parse_punctuated_within_parenthesis<T>(input: ParseStream) -> syn::Result<Punctuated<T, Comma>>
    where
        T: Parse,
    {
        let content;
        parenthesized!(content in input);
        Punctuated::<T, Comma>::parse_terminated(&content)
    }

    pub fn parse_bool_or_true(input: ParseStream) -> syn::Result<bool> {
        if input.peek(Token![=]) && input.peek2(LitBool) {
            input.parse::<Token![=]>()?;

            Ok(input.parse::<LitBool>()?.value())
        } else {
            Ok(true)
        }
    }

    /// Parse `json!(...)` as a [`TokenStream`].
    pub fn parse_json_token_stream(input: ParseStream) -> syn::Result<TokenStream> {
        if input.peek(syn::Ident) && input.peek2(Token![!]) {
            input.parse::<Ident>().and_then(|ident| {
                if ident != "json" {
                    return Err(Error::new(
                        ident.span(),
                        format!("unexpected token {ident}, expected: json!(...)"),
                    ));
                }

                Ok(ident)
            })?;
            input.parse::<Token![!]>()?;

            Ok(input.parse::<Group>()?.stream())
        } else {
            Err(Error::new(input.span(), "unexpected token, expected json!(...)"))
        }
    }
}
