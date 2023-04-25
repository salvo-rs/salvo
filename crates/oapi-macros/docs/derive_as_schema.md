
Generate reusable OpenAPI schema to be used
together with [`OpenApi`][openapi_derive].

This is `#[derive]` implementation for [`AsSchema`][as_schema] trait. The macro accepts one
`schema`
attribute optionally which can be used to enhance generated documentation. The attribute can be placed
at item level or field level in struct and enums. Currently placing this attribute to unnamed field does
not have any effect.

You can use the Rust's own `#[deprecated]` attribute on any struct, enum or field to mark it as deprecated and it will
reflect to the generated OpenAPI spec.

`#[deprecated]` attribute supports adding additional details such as a reason and or since version but this is is not supported in
OpenAPI. OpenAPI has only a boolean flag to determine deprecation. While it is totally okay to declare deprecated with reason
`#[deprecated  = "There is better way to do this"]` the reason would not render in OpenAPI spec.

Doc comments on fields will resolve to field descriptions in generated OpenAPI doc. On struct
level doc comments will resolve to object descriptions.

```
/// This is a pet
#[derive(salvo_oapi::AsSchema)]
struct Pet {
    /// Name for your pet
    name: String,
}
```

# Struct Optional Configuration Options for `#[schema(...)]`
* `example = ...` Can be _`json!(...)`_. _`json!(...)`_ should be something that
  _`serde_json::json!`_ can parse as a _`serde_json::Value`_.
* `xml(...)` Can be used to define [`Xml`][xml] object properties applicable to Structs.
* `title = ...` Literal string value. Can be used to define title for struct in OpenAPI
  document. Some OpenAPI code generation libraries also use this field as a name for the
  struct.
* `rename_all = ...` Supports same syntax as _serde_ _`rename_all`_ attribute. Will rename all fields
  of the structs accordingly. If both _serde_ `rename_all` and _schema_ _`rename_all`_ are defined
  __serde__ will take precedence.
* `as = ...` Can be used to define alternative path and name for the schema what will be used in
  the OpenAPI. E.g _`as = path::to::Pet`_. This would make the schema appear in the generated
  OpenAPI spec as _`path.to.Pet`_.
* `default` Can be used to populate default values on all fields using the struct's
  [`Default`](std::default::Default) implementation.

# Enum Optional Configuration Options for `#[schema(...)]`
* `example = ...` Can be method reference or _`json!(...)`_.
* `default = ...` Can be method reference or _`json!(...)`_.
* `title = ...` Literal string value. Can be used to define title for enum in OpenAPI
  document. Some OpenAPI code generation libraries also use this field as a name for the
  enum. __Note!__  ___Complex enum (enum with other than unit variants) does not support title!___
* `rename_all = ...` Supports same syntax as _serde_ _`rename_all`_ attribute. Will rename all
  variants of the enum accordingly. If both _serde_ `rename_all` and _schema_ _`rename_all`_
  are defined __serde__ will take precedence.
* `as = ...` Can be used to define alternative path and name for the schema what will be used in
  the OpenAPI. E.g _`as = path::to::Pet`_. This would make the schema appear in the generated
  OpenAPI spec as _`path.to.Pet`_.

# Enum Variant Optional Configuration Options for `#[schema(...)]`
Supports all variant specific configuration options e.g. if variant is _`UnnamedStruct`_ then
unnamed struct type configuration options are supported.

In addition to the variant type specific configuration options enum variants support custom
_`rename`_ attribute. It behaves similarly to serde's _`rename`_ attribute. If both _serde_
_`rename`_ and _schema_ _`rename`_ are defined __serde__ will take precedence.

# Unnamed Field Struct Optional Configuration Options for `#[schema(...)]`
* `example = ...` Can be method reference or _`json!(...)`_.
* `default = ...` Can be method reference or _`json!(...)`_. If no value is specified, and the struct has
  only one field, the field's default value in the schema will be set from the struct's
  [`Default`](std::default::Default) implementation.
* `format = ...` May either be variant of the [`KnownFormat`][known_format] enum, or otherwise
  an open value as a string. By default the format is derived from the type of the property
  according OpenApi spec.
* `value_type = ...` Can be used to override default type derived from type of the field used in OpenAPI spec.
  This is useful in cases where the default type does not correspond to the actual type e.g. when
  any third-party types are used which are not [`AsSchema`][as_schema]s nor [`primitive` types][primitive].
   Value can be any Rust type what normally could be used to serialize to JSON or custom type such as _`Object`_.
   _`Object`_ will be rendered as generic OpenAPI object _(`type: object`)_.
* `title = ...` Literal string value. Can be used to define title for struct in OpenAPI
  document. Some OpenAPI code generation libraries also use this field as a name for the
  struct.
* `as = ...` Can be used to define alternative path and name for the schema what will be used in
  the OpenAPI. E.g _`as = path::to::Pet`_. This would make the schema appear in the generated
  OpenAPI spec as _`path.to.Pet`_.

# Named Fields Optional Configuration Options for `#[schema(...)]`
* `example = ...` Can be method reference or _`json!(...)`_.
* `default = ...` Can be method reference or _`json!(...)`_.
* `format = ...` May either be variant of the [`KnownFormat`][known_format] enum, or otherwise
  an open value as a string. By default the format is derived from the type of the property
  according OpenApi spec.
* `write_only` Defines property is only used in **write** operations *POST,PUT,PATCH* but not in *GET*
* `read_only` Defines property is only used in **read** operations *GET* but not in *POST,PUT,PATCH*
* `value_type = ...` Can be used to override default type derived from type of the field used in OpenAPI spec.
  This is useful in cases where the default type does not correspond to the actual type e.g. when
  any third-party types are used which are not [`AsSchema`][as_schema]s nor [`primitive` types][primitive].
   Value can be any Rust type what normally could be used to serialize to JSON or custom type such as _`Object`_.
   _`Object`_ will be rendered as generic OpenAPI object _(`type: object`)_.
* `inline` If the type of this field implements [`AsSchema`][as_schema], then the schema definition
  will be inlined. **warning:** Don't use this for recursive data types!
* `required = ...` Can be used to enforce required status for the field. [See
  rules][derive@AsSchema#field-nullability-and-required-rules]
* `nullable` Defines property is nullable (note this is different to non-required).
* `rename = ...` Supports same syntax as _serde_ _`rename`_ attribute. Will rename field
  accordingly. If both _serde_ `rename` and _schema_ _`rename`_ are defined __serde__ will take
  precedence.
* `multiple_of = ...` Can be used to define multiplier for a value. Value is considered valid
  division will result an `integer`. Value must be strictly above _`0`_.
* `maximum = ...` Can be used to define inclusive upper bound to a `number` value.
* `minimum = ...` Can be used to define inclusive lower bound to a `number` value.
* `exclusive_maximum = ...` Can be used to define exclusive upper bound to a `number` value.
* `exclusive_minimum = ...` Can be used to define exclusive lower bound to a `number` value.
* `max_length = ...` Can be used to define maximum length for `string` types.
* `min_length = ...` Can be used to define minimum length for `string` types.
* `pattern = ...` Can be used to define valid regular expression in _ECMA-262_ dialect the field value must match.
* `max_items = ...` Can be used to define maximum items allowed for `array` fields. Value must
  be non-negative integer.
* `min_items = ...` Can be used to define minimum items allowed for `array` fields. Value must
  be non-negative integer.
* `with_schema = ...` Use _`schema`_ created by provided function reference instead of the
  default derived _`schema`_. The function must match to `fn() -> Into<RefOr<Schema>>`. It does
  not accept arguments and must return anything that can be converted into `RefOr<Schema>`.
* `additional_properties = ...` Can be used to define free form types for maps such as
  [`HashMap`](std::collections::HashMap) and [`BTreeMap`](std::collections::BTreeMap).
  Free form type enables use of arbitrary types within map values.
  Supports formats _`additional_properties`_ and _`additional_properties = true`_.

#### Field nullability and required rules

Field is considered _`required`_ if
* it is not `Option` field
* and it does not have _`skip_serializing_if`_ property
* and it does not have _`serde_with`_ _[`double_option`](https://docs.rs/serde_with/latest/serde_with/rust/double_option/index.html)_
* and it does not have default value provided with serde _`default`_
  attribute

Field is considered _`nullable`_ when field type is _`Option`_.

## Xml attribute Configuration Options

* `xml(name = "...")` Will set name for property or type.
* `xml(namespace = "...")` Will set namespace for xml element which needs to be valid uri.
* `xml(prefix = "...")` Will set prefix for name.
* `xml(attribute)` Will translate property to xml attribute instead of xml element.
* `xml(wrapped)` Will make wrapped xml element.
* `xml(wrapped(name = "wrap_name"))` Will override the wrapper elements name.

See [`Xml`][xml] for more details.

# Partial `#[serde(...)]` attributes support

`AsSchema` derive has partial support for [serde attributes]. These supported attributes will reflect to the
generated OpenAPI doc. For example if _`#[serde(skip)]`_ is defined the attribute will not show up in the OpenAPI spec at all since it will not never
be serialized anyway. Similarly the _`rename`_ and _`rename_all`_ will reflect to the generated OpenAPI doc.

* `rename_all = "..."` Supported at the container level.
* `rename = "..."` Supported **only** at the field or variant level.
* `skip = "..."` Supported  **only** at the field or variant level.
* `skip_serializing = "..."` Supported  **only** at the field or variant level.
* `skip_serializing_if = "..."` Supported  **only** at the field level.
* `with = ...` Supported **only at field level.**
* `tag = "..."` Supported at the container level. `tag` attribute works as a [discriminator field][discriminator] for an enum.
* `content = "..."` Supported at the container level, allows [adjacently-tagged enums](https://serde.rs/enum-representations.html#adjacently-tagged).
  This attribute requires that a `tag` is present, otherwise serde will trigger a compile-time
  failure.
* `untagged` Supported at the container level. Allows [untagged
enum representation](https://serde.rs/enum-representations.html#untagged).
* `default` Supported at the container level and field level according to [serde attributes].
* `flatten` Supported at the field level.

Other _`serde`_ attributes works as is but does not have any effect on the generated OpenAPI doc.

**Note!** `tag` attribute has some limitations like it cannot be used
with **unnamed field structs** and **tuple types**.  See more at
[enum representation docs](https://serde.rs/enum-representations.html).

**Note!** `with` attribute is used in tandem with [serde_with](https://github.com/jonasbb/serde_with) to recognize
_[`double_option`](https://docs.rs/serde_with/latest/serde_with/rust/double_option/index.html)_ from **field value**.
_`double_option`_ is **only** supported attribute from _`serde_with`_ crate.

```
# use serde::Serialize;
# use salvo_oapi::AsSchema;
#[derive(Serialize, AsSchema)]
struct Foo(String);

#[derive(Serialize, AsSchema)]
#[serde(rename_all = "camelCase")]
enum Bar {
    UnitValue,
    #[serde(rename_all = "camelCase")]
    NamedFields {
        #[serde(rename = "id")]
        named_id: &'static str,
        name_list: Option<Vec<String>>
    },
    UnnamedFields(Foo),
    #[serde(skip)]
    SkipMe,
}
```

_**Add custom `tag` to change JSON representation to be internally tagged.**_
```
# use serde::Serialize;
# use salvo_oapi::AsSchema;
#[derive(Serialize, AsSchema)]
struct Foo(String);

#[derive(Serialize, AsSchema)]
#[serde(tag = "tag")]
enum Bar {
    UnitValue,
    NamedFields {
        id: &'static str,
        names: Option<Vec<String>>
    },
}
```

_**Add serde `default` attribute for MyValue struct. Similarly `default` could be added to
individual fields as well. If `default` is given the field's affected will be treated
as optional.**_
```
 #[derive(salvo_oapi::AsSchema, serde::Deserialize, Default)]
 #[serde(default)]
 struct MyValue {
     field: String
 }
```

# `#[repr(...)]` attribute support

[Serde repr](https://github.com/dtolnay/serde-repr) allows field-less enums be represented by
their numeric value.

* `repr(u*)` for unsigned integer.
* `repr(i*)` for signed integer.

**Supported schema attributes**

* `example = ...` Can be method reference or _`json!(...)`_.
* `default = ...` Can be method reference or _`json!(...)`_.
* `title = ...` Literal string value. Can be used to define title for enum in OpenAPI
  document. Some OpenAPI code generation libraries also use this field as a name for the
  enum. __Note!__  ___Complex enum (enum with other than unit variants) does not support title!___
* `as = ...` Can be used to define alternative path and name for the schema what will be used in
  the OpenAPI. E.g _`as = path::to::Pet`_. This would make the schema appear in the generated
  OpenAPI spec as _`path.to.Pet`_.

_**Create enum with numeric values.**_
```
# use salvo_oapi::AsSchema;
#[derive(AsSchema)]
#[repr(u8)]
#[schema(default = default_value, example = 2)]
enum Mode {
    One = 1,
    Two,
 }

fn default_value() -> u8 {
    1
}
```

_**You can use `skip` and `tag` attributes from serde.**_
```
# use salvo_oapi::AsSchema;
#[derive(AsSchema, serde::Serialize)]
#[repr(i8)]
#[serde(tag = "code")]
enum ExitCode {
    Error = -1,
    #[serde(skip)]
    Unknown = 0,
    Ok = 1,
 }
```

# Generic schemas with aliases

Schemas can also be generic which allows reusing types. This enables certain behaviour patters
where super type declares common code for type aliases.

In this example we have common `Status` type which accepts one generic type. It is then defined
with `#[aliases(...)]` that it is going to be used with [`String`](std::string::String) and [`i32`] values.
The generic argument could also be another [`AsSchema`][as_schema] as well.
```
# use salvo_oapi::{AsSchema, OpenApi};
#[derive(AsSchema)]
#[aliases(StatusMessage = Status<String>, StatusNumber = Status<i32>)]
struct Status<T> {
    value: T
}

```

The `#[aliases(...)]` is just syntactic sugar and will create Rust [type aliases](https://doc.rust-lang.org/reference/items/type-aliases.html)
behind the scenes which then can be later referenced anywhere in code.

**Note!** You should never register generic type itself in `components(...)` so according above example `Status<...>` should not be registered
because it will not render the type correctly and will cause an error in generated OpenAPI spec.

# Examples

_**Simple example of a Pet with descriptions and object level example.**_
```
# use salvo_oapi::AsSchema;
/// This is a pet.
#[derive(AsSchema)]
#[schema(example = json!({"name": "bob the cat", "id": 0}))]
struct Pet {
    /// Unique id of a pet.
    id: u64,
    /// Name of a pet.
    name: String,
    /// Age of a pet if known.
    age: Option<i32>,
}
```

_**The `schema` attribute can also be placed at field level as follows.**_
```
# use salvo_oapi::AsSchema;
#[derive(AsSchema)]
struct Pet {
    #[schema(example = 1, default = 0)]
    id: u64,
    name: String,
    age: Option<i32>,
}
```

_**You can also use method reference for attribute values.**_
```
# use salvo_oapi::AsSchema;
#[derive(AsSchema)]
struct Pet {
    #[schema(example = u64::default, default = u64::default)]
    id: u64,
    #[schema(default = default_name)]
    name: String,
    age: Option<i32>,
}

fn default_name() -> String {
    "bob".to_string()
}
```

_**For enums and unnamed field structs you can define `schema` at type level.**_
```
# use salvo_oapi::AsSchema;
#[derive(AsSchema)]
#[schema(example = "Bus")]
enum VehicleType {
    Rocket, Car, Bus, Submarine
}
```

_**Also you write complex enum combining all above types.**_
```
# use salvo_oapi::AsSchema;
#[derive(AsSchema)]
enum ErrorResponse {
    InvalidCredentials,
    #[schema(default = String::default, example = "Pet not found")]
    NotFound(String),
    System {
        #[schema(example = "Unknown system failure")]
        details: String,
    }
}
```

_**It is possible to specify the title of each variant to help generators create named structures.**_
```
# use salvo_oapi::AsSchema;
#[derive(AsSchema)]
enum ErrorResponse {
    #[schema(title = "InvalidCredentials")]
    InvalidCredentials,
    #[schema(title = "NotFound")]
    NotFound(String),
}
```

_**Use `xml` attribute to manipulate xml output.**_
```
# use salvo_oapi::AsSchema;
#[derive(AsSchema)]
#[schema(xml(name = "user", prefix = "u", namespace = "https://user.xml.schema.test"))]
struct User {
    #[schema(xml(attribute, prefix = "u"))]
    id: i64,
    #[schema(xml(name = "user_name", prefix = "u"))]
    username: String,
    #[schema(xml(wrapped(name = "linkList"), name = "link"))]
    links: Vec<String>,
    #[schema(xml(wrapped, name = "photo_url"))]
    photos_urls: Vec<String>
}
```

_**Use of Rust's own `#[deprecated]` attribute will reflect to generated OpenAPI spec.**_
```
# use salvo_oapi::AsSchema;
#[derive(AsSchema)]
#[deprecated]
struct User {
    id: i64,
    username: String,
    links: Vec<String>,
    #[deprecated]
    photos_urls: Vec<String>
}
```

_**Enforce type being used in OpenAPI spec to [`String`] with `value_type` and set format to octet stream
with [`SchemaFormat::KnownFormat(KnownFormat::Binary)`][binary].**_
```
# use salvo_oapi::AsSchema;
#[derive(AsSchema)]
struct Post {
    id: i32,
    #[schema(value_type = String, format = Binary)]
    value: Vec<u8>,
}
```

_**Enforce type being used in OpenAPI spec to [`String`] with `value_type` option.**_
```
# use salvo_oapi::AsSchema;
#[derive(AsSchema)]
#[schema(value_type = String)]
struct Value(i64);
```

_**Override the `Bar` reference with a `custom::NewBar` reference.**_
```
# use salvo_oapi::AsSchema;
#  mod custom {
#      struct NewBar;
#  }
#
# struct Bar;
#[derive(AsSchema)]
struct Value {
    #[schema(value_type = custom::NewBar)]
    field: Bar,
};
```

_**Use a virtual `Object` type to render generic `object` _(`type: object`)_ in OpenAPI spec.**_
```
# use salvo_oapi::AsSchema;
# mod custom {
#    struct NewBar;
# }
#
# struct Bar;
#[derive(AsSchema)]
struct Value {
    #[schema(value_type = Object)]
    field: Bar,
};
```

_**Serde `rename` / `rename_all` will take precedence over schema `rename` / `rename_all`.**_
```
#[derive(salvo_oapi::AsSchema, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[schema(rename_all = "UPPERCASE")]
enum Random {
    #[serde(rename = "string_value")]
    #[schema(rename = "custom_value")]
    String(String),

    Number {
        id: i32,
    }
}
```

_**Add `title` to the enum.**_
```
#[derive(salvo_oapi::AsSchema)]
#[schema(title = "UserType")]
enum UserType {
    Admin,
    Moderator,
    User,
}
```

_**Example with validation attributes.**_
```
#[derive(salvo_oapi::AsSchema, serde::Deserialize)]
struct Item {
    #[schema(maximum = 10, minimum = 5, multiple_of = 2.5)]
    id: i32,
    #[schema(max_length = 10, min_length = 5, pattern = "[a-z]*")]
    value: String,
    #[schema(max_items = 5, min_items = 1)]
    items: Vec<String>,
}
````

_**Use `schema_with` to manually implement schema for a field.**_
```
# use salvo_oapi::schema::Object;
fn custom_type() -> Object {
    Object::new()
        .schema_type(salvo_oapi::SchemaType::String)
        .format(salvo_oapi::SchemaFormat::Custom(
            "email".to_string(),
        ))
        .description("this is the description")
}

#[derive(salvo_oapi::AsSchema)]
struct Value {
    #[schema(schema_with = custom_type)]
    id: String,
}
```

_**Use `as` attribute to change the name and the path of the schema in the generated OpenAPI
spec.**_
```
 #[derive(salvo_oapi::AsSchema)]
 #[schema(as = api::models::person::Person)]
 struct Person {
     name: String,
 }
```

More examples for _`value_type`_ in [`AsParameters` derive docs][parameters].

[as_schema]: trait.AsSchema.html
[known_format]: openapi/schema/enum.KnownFormat.html
[binary]: openapi/schema/enum.KnownFormat.html#variant.Binary
[xml]: openapi/xml/struct.Xml.html
[as_parameters]: derive.AsParameters.html
[primitive]: https://doc.rust-lang.org/std/primitive/index.html
[serde attributes]: https://serde.rs/attributes.html
[discriminator]: openapi/schema/struct.Discriminator.html
[enum_schema]: derive.AsSchema.html#enum-optional-configuration-options-for-schema
[openapi_derive]: derive.OpenApi.html