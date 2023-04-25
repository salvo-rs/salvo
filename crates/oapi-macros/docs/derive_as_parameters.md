Generate [path parameters][path_params] from struct's
fields.

This is `#[derive]` implementation for [`AsParameters`][as_parameters] trait.

Typically path parameters need to be defined within [`#[salvo_oapi::endpoint(...parameters(...))]`][path_params] section
for the endpoint. But this trait eliminates the need for that when [`struct`][struct]s are used to define parameters.
Still [`std::primitive`] and [`String`](std::string::String) path parameters or [`tuple`] style path parameters need to be defined
within `parameters(...)` section if description or other than default configuration need to be given.

You can use the Rust's own `#[deprecated]` attribute on field to mark it as
deprecated and it will reflect to the generated OpenAPI spec.

`#[deprecated]` attribute supports adding additional details such as a reason and or since version
but this is is not supported in OpenAPI. OpenAPI has only a boolean flag to determine deprecation.
While it is totally okay to declare deprecated with reason
`#[deprecated  = "There is better way to do this"]` the reason would not render in OpenAPI spec.

Doc comment on struct fields will be used as description for the generated parameters.
```
#[derive(salvo_oapi::AsParameters, serde::Deserialize)]
struct Query {
    /// Query todo items by name.
    name: String
}
```

# AsParameters Container Attributes for `#[parameters(...)]`

The following attributes are available for use in on the container attribute `#[parameters(...)]` for the struct
deriving `AsParameters`:

* `names(...)` Define comma separated list of names for unnamed fields of struct used as a path parameter.
   __Only__ supported on __unnamed structs__.
* `style = ...` Defines how all parameters are serialized by [`ParameterStyle`][style]. Default
   values are based on _`parameter_in`_ attribute.
* `parameter_in = ...` =  Defines where the parameters of this field are used with a value from
   [`parameter::ParameterIn`][in_enum]. There is no default value, if this attribute is not
   supplied, then the value is determined by the `parameter_in_provider` in
   [`AsParameters::parameters()`](trait.AsParameters.html#tymethod.parameters).
* `rename_all = ...` Can be provided to alternatively to the serde's `rename_all` attribute. Effectively provides same functionality.

Use `names` to define name for single unnamed argument.
```
# use salvo_oapi::AsParameters;
#
#[derive(AsParameters, serde::Deserialize)]
#[as_parameters(names("id"))]
struct Id(u64);
```

Use `names` to define names for multiple unnamed arguments.
```
# use salvo_oapi::AsParameters;
#
#[derive(AsParameters, serde::Deserialize)]
#[as_parameters(names("id", "name"))]
struct IdAndName(u64, String);
```

# AsParameters Field Attributes for `#[parameter(...)]`

The following attributes are available for use in the `#[parameter(...)]` on struct fields:

* `style = ...` Defines how the parameter is serialized by [`ParameterStyle`][style]. Default values are based on _`parameter_in`_ attribute.

* `explode` Defines whether new _`parameter=value`_ pair is created for each parameter withing _`object`_ or _`array`_.

* `allow_reserved` Defines whether reserved characters _`:/?#[]@!$&'()*+,;=`_ is allowed within value.

* `example = ...` Can be method reference or _`json!(...)`_. Given example
  will override any example in underlying parameter type.

* `value_type = ...` Can be used to override default type derived from type of the field used in OpenAPI spec.
  This is useful in cases where the default type does not correspond to the actual type e.g. when
  any third-party types are used which are not [`AsSchema`][as_schema]s nor [`primitive` types][primitive].
   Value can be any Rust type what normally could be used to serialize to JSON or custom type such as _`Object`_.
   _`Object`_ will be rendered as generic OpenAPI object.

* `inline` If set, the schema for this field's type needs to be a [`AsSchema`][as_schema], and
  the schema definition will be inlined.

* `default = ...` Can be method reference or _`json!(...)`_.

* `format = ...` May either be variant of the [`KnownFormat`][known_format] enum, or otherwise
  an open value as a string. By default the format is derived from the type of the property
  according OpenApi spec.

* `write_only` Defines property is only used in **write** operations *POST,PUT,PATCH* but not in *GET*

* `read_only` Defines property is only used in **read** operations *GET* but not in *POST,PUT,PATCH*

* `xml(...)` Can be used to define [`Xml`][xml] object properties applicable to named fields.
   See configuration options at xml attributes of [`AsSchema`][as_schema_xml]

* `nullable` Defines property is nullable (note this is different to non-required).

* `required = ...` Can be used to enforce required status for the parameter. [See
   rules][derive@AsParameters#field-nullability-and-required-rules]

* `rename = ...` Can be provided to alternatively to the serde's `rename` attribute. Effectively provides same functionality.

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

Same rules for nullability and required status apply for _`AsParameters`_ field attributes as for
_`AsSchema`_ field attributes. [See the rules][`derive@AsSchema#field-nullability-and-required-rules`].

# Partial `#[serde(...)]` attributes support

AsParameters derive has partial support for [serde attributes]. These supported attributes will reflect to the
generated OpenAPI doc. The following attributes are currently supported:

* `rename_all = "..."` Supported at the container level.
* `rename = "..."` Supported **only** at the field level.
* `default` Supported at the container level and field level according to [serde attributes].
* `skip_serializing_if = "..."` Supported  **only** at the field level.
* `with = ...` Supported **only** at field level.

Other _`serde`_ attributes will impact the serialization but will not be reflected on the generated OpenAPI doc.

# Examples

_**Demonstrate [`AsParameters`][as_parameters] usage with the `#[as_parameters(...)]` container attribute to
be used as a path query, and inlining a schema query field:**_

```
use serde::Deserialize;
use salvo_core::prelude::*;
use salvo_oapi::{AsParameters, AsSchema};

#[derive(Deserialize, AsSchema)]
#[serde(rename_all = "snake_case")]
enum PetKind {
    Dog,
    Cat,
}

#[derive(Deserialize, AsParameters)]
struct PetQuery {
    /// Name of pet
    name: Option<String>,
    /// Age of pet
    age: Option<i32>,
    /// Kind of pet
    #[parameter(inline)]
    kind: PetKind
}

#[salvo_oapi::endpoint(
    parameters(PetQuery),
    responses(
        (status = 200, description = "success response")
    )
)]
async fn get_pet(query: PetQuery) {
    // ...
}
```

_**Override `String` with `i64` using `value_type` attribute.**_
```
# use salvo_oapi::AsParameters;
#
#[derive(AsParameters, serde::Deserialize)]
#[as_parameters(parameter_in = Query)]
struct Filter {
    #[parameter(value_type = i64)]
    id: String,
}
```

_**Override `String` with `Object` using `value_type` attribute. _`Object`_ will render as `type: object` in OpenAPI spec.**_
```
# use salvo_oapi::AsParameters;
#
#[derive(AsParameters, serde::Deserialize)]
#[as_parameters(parameter_in = Query)]
struct Filter {
    #[parameter(value_type = Object)]
    id: String,
}
```

_**You can use a generic type to override the default type of the field.**_
```
# use salvo_oapi::AsParameters;
#
#[derive(AsParameters, serde::Deserialize)]
#[as_parameters(parameter_in = Query)]
struct Filter {
    #[parameter(value_type = Option<String>)]
    id: String
}
```

_**You can even override a [`Vec`] with another one.**_
```
# use salvo_oapi::AsParameters;
#
#[derive(AsParameters, serde::Deserialize)]
#[as_parameters(parameter_in = Query)]
struct Filter {
    #[parameter(value_type = Vec<i32>)]
    id: Vec<String>
}
```

_**We can override value with another [`AsSchema`][as_schema].**_
```
# use salvo_oapi::{AsParameters, AsSchema};
#
#[derive(AsSchema)]
struct Id {
    value: i64,
}

#[derive(AsParameters, serde::Deserialize)]
#[as_parameters(parameter_in = Query)]
struct Filter {
    #[parameter(value_type = Id)]
    id: String
}
```

_**Example with validation attributes.**_
```
#[derive(salvo_oapi::AsParameters, serde::Deserialize)]
struct Item {
    #[parameter(maximum = 10, minimum = 5, multiple_of = 2.5)]
    id: i32,
    #[parameter(max_length = 10, min_length = 5, pattern = "[a-z]*")]
    value: String,
    #[parameter(max_items = 5, min_items = 1)]
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

#[derive(salvo_oapi::AsParameters, serde::Deserialize)]
#[as_parameters(parameter_in = Query)]
struct Query {
    #[parameter(schema_with = custom_type)]
    email: String,
}
```

[as_schema]: trait.AsSchema.html
[known_format]: openapi/schema/enum.KnownFormat.html
[xml]: openapi/xml/struct.Xml.html
[as_parameters]: trait.AsParameters.html
[path_params]: attr.path.html#params-attributes
[struct]: https://doc.rust-lang.org/std/keyword.struct.html
[style]: openapi/path/enum.ParameterStyle.html
[in_enum]: salvo_oapi/openapi/path/enum.ParameterIn.html
[primitive]: https://doc.rust-lang.org/std/primitive/index.html
[serde attributes]: https://serde.rs/attributes.html