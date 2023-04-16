//! Implements [OpenAPI Schema Object][schema] types which can be
//! used to define field properties, enum values, array or object types.
//!
//! [schema]: https://spec.openapis.org/oas/latest.html#schema-object
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::RefOr;
use super::{security::SecurityScheme, set_value, xml::Xml, Deprecated, Response};
use crate::{ToResponse, ToSchema};

/// Create an _`empty`_ [`Schema`] that serializes to _`null`_.
///
/// Can be used in places where an item can be serialized as `null`. This is used with unit type
/// enum variants and tuple unit types.
pub fn empty() -> Schema {
    Schema::Object(
        Object::new()
            .nullable(true)
            .default_value(serde_json::Value::Null)
            .into(),
    )
}

/// Implements [OpenAPI Components Object][components] which holds supported
/// reusable objects.
///
/// Components can hold either reusable types themselves or references to other reusable
/// types.
///
/// [components]: https://spec.openapis.org/oas/latest.html#components-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Components {
    /// Map of reusable [OpenAPI Schema Object][schema]s.
    ///
    /// [schema]: https://spec.openapis.org/oas/latest.html#schema-object
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub schemas: BTreeMap<String, RefOr<Schema>>,

    /// Map of reusable response name, to [OpenAPI Response Object][response]s or [OpenAPI
    /// Reference][reference]s to [OpenAPI Response Object][response]s.
    ///
    /// [response]: https://spec.openapis.org/oas/latest.html#response-object
    /// [reference]: https://spec.openapis.org/oas/latest.html#reference-object
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub responses: BTreeMap<String, RefOr<Response>>,

    /// Map of reusable [OpenAPI Security Scheme Object][security_scheme]s.
    ///
    /// [security_scheme]: https://spec.openapis.org/oas/latest.html#security-scheme-object
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub security_schemes: BTreeMap<String, SecurityScheme>,
}

impl Components {
    /// Construct a new [`Components`].
    pub fn new() -> Self {
        Self { ..Default::default() }
    }
    /// Add [`SecurityScheme`] to [`Components`]
    ///
    /// Accepts two arguments where first is the name of the [`SecurityScheme`]. This is later when
    /// referenced by [`SecurityRequirement`][requirement]s. Second parameter is the [`SecurityScheme`].
    ///
    /// [requirement]: ../security/struct.SecurityRequirement.html
    pub fn add_security_scheme<N: Into<String>, S: Into<SecurityScheme>>(&mut self, name: N, security_schema: S) {
        self.security_schemes.insert(name.into(), security_schema.into());
    }

    /// Add iterator of [`SecurityScheme`]s to [`Components`].
    ///
    /// Accepts two arguments where first is the name of the [`SecurityScheme`]. This is later when
    /// referenced by [`SecurityRequirement`][requirement]s. Second parameter is the [`SecurityScheme`].
    ///
    /// [requirement]: ../security/struct.SecurityRequirement.html
    pub fn add_security_schemes_from_iter<I: IntoIterator<Item = (N, S)>, N: Into<String>, S: Into<SecurityScheme>>(
        &mut self,
        schemas: I,
    ) {
        self.security_schemes
            .extend(schemas.into_iter().map(|(name, item)| (name.into(), item.into())));
    }

    /// Add [`Schema`] to [`Components`].
    ///
    /// Accepts two arguments where first is name of the schema and second is the schema itself.
    pub fn schema<S: Into<String>, I: Into<RefOr<Schema>>>(mut self, name: S, schema: I) -> Self {
        self.schemas.insert(name.into(), schema.into());

        self
    }

    pub fn schema_from<'s, I: ToSchema<'s>>(mut self) -> Self {
        let aliases = I::aliases();

        // TODO a temporal hack to add the main schema only if there are no aliases pre-defined.
        // Eventually aliases functionality should be extracted out from the `ToSchema`. Aliases
        // are created when the main schema is a generic type which should be included in OpenAPI
        // spec in its generic form.
        if aliases.is_empty() {
            let (name, schema) = I::schema();
            self.schemas.insert(name.to_string(), schema);
        }

        self.schemas_from_iter(aliases)
    }

    /// Add [`Schema`]s from iterator.
    ///
    /// # Examples
    /// ```
    /// # use salvo_oapi::openapi::schema::{Components, Object,
    /// #    SchemaType, Schema};
    /// Components::new().schemas_from_iter([(
    ///     "Pet",
    ///     Schema::from(
    ///         Object::new()
    ///             .property(
    ///                 "name",
    ///                 Object::new().schema_type(SchemaType::String),
    ///             )
    ///             .required("name")
    ///     ),
    /// )]);
    /// ```
    pub fn schemas_from_iter<I: IntoIterator<Item = (S, C)>, C: Into<RefOr<Schema>>, S: Into<String>>(
        mut self,
        schemas: I,
    ) -> Self {
        self.schemas
            .extend(schemas.into_iter().map(|(name, schema)| (name.into(), schema.into())));

        self
    }

    pub fn response<S: Into<String>, R: Into<RefOr<Response>>>(mut self, name: S, response: R) -> Self {
        self.responses.insert(name.into(), response.into());
        self
    }

    pub fn response_from<'r, I: ToResponse<'r>>(self) -> Self {
        let (name, response) = I::response();
        self.response(name, response)
    }

    pub fn responses_from_iter<I: IntoIterator<Item = (S, R)>, S: Into<String>, R: Into<RefOr<Response>>>(
        mut self,
        responses: I,
    ) -> Self {
        self.responses.extend(
            responses
                .into_iter()
                .map(|(name, response)| (name.into(), response.into())),
        );

        self
    }

    /// Add [`SecurityScheme`] to [`Components`].
    ///
    /// Accepts two arguments where first is the name of the [`SecurityScheme`]. This is later when
    /// referenced by [`SecurityRequirement`][requirement]s. Second parameter is the [`SecurityScheme`].
    ///
    /// [requirement]: ../security/struct.SecurityRequirement.html
    pub fn security_scheme<N: Into<String>, S: Into<SecurityScheme>>(mut self, name: N, security_schema: S) -> Self {
        self.security_schemes.insert(name.into(), security_schema.into());

        self
    }
}

/// Is super type for [OpenAPI Schema Object][schemas]. Schema is reusable resource what can be
/// referenced from path operations and other components using [`Ref`].
///
/// [schemas]: https://spec.openapis.org/oas/latest.html#schema-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged, rename_all = "camelCase")]
pub enum Schema {
    /// Defines array schema from another schema. Typically used with
    /// [`Schema::Object`]. Slice and Vec types are translated to [`Schema::Array`] types.
    Array(Array),
    /// Defines object schema. Object is either `object` holding **properties** which are other [`Schema`]s
    /// or can be a field within the [`Object`].
    Object(Object),
    /// Creates a _OneOf_ type [composite Object][composite] schema. This schema
    /// is used to map multiple schemas together where API endpoint could return any of them.
    /// [`Schema::OneOf`] is created form complex enum where enum holds other than unit types.
    ///
    /// [composite]: https://spec.openapis.org/oas/latest.html#components-object
    OneOf(OneOf),

    /// Creates a _AnyOf_ type [composite Object][composite] schema.
    ///
    /// [composite]: https://spec.openapis.org/oas/latest.html#components-object
    AllOf(AllOf),
}

impl Default for Schema {
    fn default() -> Self {
        Schema::Object(Object::default())
    }
}

/// OpenAPI [Discriminator][discriminator] object which can be optionally used together with
/// [`OneOf`] composite object.
///
/// [discriminator]: https://spec.openapis.org/oas/latest.html#discriminator-object
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Discriminator {
    /// Defines a discriminator property name which must be found within all composite
    /// objects.
    pub property_name: String,
}

impl Discriminator {
    /// Construct a new [`Discriminator`] object with property name.
    ///
    /// # Examples
    ///
    /// Create a new [`Discriminator`] object for `pet_type` property.
    /// ```
    /// # use salvo_oapi::openapi::schema::Discriminator;
    /// let discriminator = Discriminator::new("pet_type");
    /// ```
    pub fn new<I: Into<String>>(property_name: I) -> Self {
        Self {
            property_name: property_name.into(),
        }
    }
}

/// OneOf [Composite Object][oneof] component holds
/// multiple components together where API endpoint could return any of them.
///
/// See [`Schema::OneOf`] for more details.
///
/// [oneof]: https://spec.openapis.org/oas/latest.html#components-object
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct OneOf {
    /// Components of _OneOf_ component.
    #[serde(rename = "oneOf")]
    pub items: Vec<RefOr<Schema>>,

    /// Description of the [`OneOf`]. Markdown syntax is supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Default value which is provided when user has not provided the input in Swagger UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,

    /// Example shown in UI of the value for richer documentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,

    /// Optional discriminator field can be used to aid deserialization, serialization and validation of a
    /// specific schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<Discriminator>,

    /// Set `true` to allow `"null"` to be used as value for given type.
    #[serde(default, skip_serializing_if = "is_false")]
    pub nullable: bool,
}

impl OneOf {
    /// Construct a new [`OneOf`] component.
    pub fn new() -> Self {
        Self { ..Default::default() }
    }

    /// Construct a new [`OneOf`] component with given capacity.
    ///
    /// OneOf component is then able to contain number of components without
    /// reallocating.
    ///
    /// # Examples
    ///
    /// Create [`OneOf`] component with initial capacity of 5.
    /// ```
    /// # use salvo_oapi::openapi::schema::OneOf;
    /// let one_of = OneOf::with_capacity(5);
    /// ```
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
            ..Default::default()
        }
    }
    /// Adds a given [`Schema`] to [`OneOf`] [Composite Object][composite]
    ///
    /// [composite]: https://spec.openapis.org/oas/latest.html#components-object
    pub fn item<I: Into<RefOr<Schema>>>(mut self, component: I) -> Self {
        self.items.push(component.into());

        self
    }

    /// Add or change optional description for `OneOf` component.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        set_value!(self description Some(description.into()))
    }

    /// Add or change default value for the object which is provided when user has not provided the input in Swagger UI.
    pub fn default_value(mut self, default: Value) -> Self {
        set_value!(self default Some(default))
    }

    /// Add or change example shown in UI of the value for richer documentation.
    pub fn example(mut self, example: Value) -> Self {
        set_value!(self example Some(example))
    }

    /// Add or change discriminator field of the composite [`OneOf`] type.
    pub fn discriminator(mut self, discriminator: Option<Discriminator>) -> Self {
        set_value!(self discriminator discriminator)
    }

    /// Add or change nullable flag for [`Object`].
    pub fn nullable(mut self, nullable: bool) -> Self {
        set_value!(self nullable nullable)
    }
}

impl From<OneOf> for Schema {
    fn from(one_of: OneOf) -> Self {
        Self::OneOf(one_of)
    }
}

impl From<OneOf> for RefOr<Schema> {
    fn from(one_of: OneOf) -> Self {
        Self::T(Schema::OneOf(one_of))
    }
}

/// AllOf [Composite Object][allof] component holds
/// multiple components together where API endpoint will return a combination of all of them.
///
/// See [`Schema::AllOf`] for more details.
///
/// [allof]: https://spec.openapis.org/oas/latest.html#components-object
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct AllOf {
    /// Components of _AllOf_ component.
    #[serde(rename = "allOf")]
    pub items: Vec<RefOr<Schema>>,

    /// Description of the [`AllOf`]. Markdown syntax is supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Default value which is provided when user has not provided the input in Swagger UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,

    /// Example shown in UI of the value for richer documentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,

    /// Optional discriminator field can be used to aid deserialization, serialization and validation of a
    /// specific schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<Discriminator>,

    /// Set `true` to allow `"null"` to be used as value for given type.
    #[serde(default, skip_serializing_if = "is_false")]
    pub nullable: bool,
}

impl AllOf {
    /// Construct a new [`AllOf`] component.
    pub fn new() -> Self {
        Self { ..Default::default() }
    }

    /// Construct a new [`AllOf`] component with given capacity.
    ///
    /// AllOf component is then able to contain number of components without
    /// reallocating.
    ///
    /// # Examples
    ///
    /// Create [`AllOf`] component with initial capacity of 5.
    /// ```
    /// # use salvo_oapi::openapi::schema::AllOf;
    /// let one_of = AllOf::with_capacity(5);
    /// ```
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
            ..Default::default()
        }
    }
    /// Adds a given [`Schema`] to [`AllOf`] [Composite Object][composite]
    ///
    /// [composite]: https://spec.openapis.org/oas/latest.html#components-object
    pub fn item<I: Into<RefOr<Schema>>>(mut self, component: I) -> Self {
        self.items.push(component.into());

        self
    }

    /// Add or change optional description for `AllOf` component.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        set_value!(self description Some(description.into()))
    }

    /// Add or change default value for the object which is provided when user has not provided the input in Swagger UI.
    pub fn default_value(mut self, default: Value) -> Self {
        set_value!(self default Some(default))
    }

    /// Add or change example shown in UI of the value for richer documentation.
    pub fn example(mut self, example: Value) -> Self {
        set_value!(self example Some(example))
    }

    /// Add or change discriminator field of the composite [`AllOf`] type.
    pub fn discriminator(mut self, discriminator: Discriminator) -> Self {
        set_value!(self discriminator Some(discriminator))
    }

    /// Add or change nullable flag for [`Object`].
    pub fn nullable(mut self, nullable: bool) -> Self {
        set_value!(self nullable nullable)
    }
}

impl From<AllOf> for Schema {
    fn from(one_of: AllOf) -> Self {
        Self::AllOf(one_of)
    }
}

impl From<AllOf> for RefOr<Schema> {
    fn from(one_of: AllOf) -> Self {
        Self::T(Schema::AllOf(one_of))
    }
}

#[cfg(not(feature = "preserve_order"))]
type ObjectPropertiesMap<K, V> = BTreeMap<K, V>;
#[cfg(feature = "preserve_order")]
type ObjectPropertiesMap<K, V> = indexmap::IndexMap<K, V>;

/// Implements subset of [OpenAPI Schema Object][schema] which allows
/// adding other [`Schema`]s as **properties** to this [`Schema`].
///
/// This is a generic OpenAPI schema object which can used to present `object`, `field` or an `enum`.
///
/// [schema]: https://spec.openapis.org/oas/latest.html#schema-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Object {
    /// Type of [`Object`] e.g. [`SchemaType::Object`] for `object` and [`SchemaType::String`] for
    /// `string` types.
    #[serde(rename = "type")]
    pub schema_type: SchemaType,

    /// Changes the [`Object`] title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Additional format for detailing the schema type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<SchemaFormat>,

    /// Description of the [`Object`]. Markdown syntax is supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Default value which is provided when user has not provided the input in Swagger UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,

    /// Enum variants of fields that can be represented as `unit` type `enums`
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<Value>>,

    /// Vector of required field names.
    #[serde(skip_serializing_if = "Vec::is_empty", default = "Vec::new")]
    pub required: Vec<String>,

    /// Map of fields with their [`Schema`] types.
    ///
    /// With **preserve_order** feature flag [`indexmap::IndexMap`] will be used as
    /// properties map backing implementation to retain property order of [`ToSchema`][to_schema].
    /// By default [`BTreeMap`] will be used.
    ///
    /// [to_schema]: crate::ToSchema
    #[serde(
        skip_serializing_if = "ObjectPropertiesMap::is_empty",
        default = "ObjectPropertiesMap::new"
    )]
    pub properties: ObjectPropertiesMap<String, RefOr<Schema>>,

    /// Additional [`Schema`] for non specified fields (Useful for typed maps).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<Box<AdditionalProperties<Schema>>>,

    /// Changes the [`Object`] deprecated status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Deprecated>,

    /// Example shown in UI of the value for richer documentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,

    /// Write only property will be only sent in _write_ requests like _POST, PUT_.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write_only: Option<bool>,

    /// Read only property will be only sent in _read_ requests like _GET_.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,

    /// Additional [`Xml`] formatting of the [`Object`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xml: Option<Xml>,

    /// Set `true` to allow `"null"` to be used as value for given type.
    #[serde(default, skip_serializing_if = "is_false")]
    pub nullable: bool,

    /// Must be a number strictly greater than `0`. Numeric value is considered valid if value
    /// divided by the _`multiple_of`_ value results an integer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiple_of: Option<f64>,

    /// Specify inclusive upper limit for the [`Object`]'s value. Number is considered valid if
    /// it is equal or less than the _`maximum`_.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,

    /// Specify inclusive lower limit for the [`Object`]'s value. Number value is considered
    /// valid if it is equal or greater than the _`minimum`_.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,

    /// Specify exclusive upper limit for the [`Object`]'s value. Number value is considered
    /// valid if it is strictly less than _`exclusive_maximum`_.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusive_maximum: Option<f64>,

    /// Specify exclusive lower limit for the [`Object`]'s value. Number value is considered
    /// valid if it is strictly above the _`exclusive_minimum`_.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusive_minimum: Option<f64>,

    /// Specify maximum length for `string` values. _`max_length`_ cannot be a negative integer
    /// value. Value is considered valid if content length is equal or less than the _`max_length`_.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,

    /// Specify minimum length for `string` values. _`min_length`_ cannot be a negative integer
    /// value. Setting this to _`0`_ has the same effect as omitting this field. Value is
    /// considered valid if content length is equal or more than the _`min_length`_.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<usize>,

    /// Define a valid `ECMA-262` dialect regular expression. The `string` content is
    /// considered valid if the _`pattern`_ matches the value successfully.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    /// Specify inclusive maximum amount of properties an [`Object`] can hold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_properties: Option<usize>,

    /// Specify inclusive minimum amount of properties an [`Object`] can hold. Setting this to
    /// `0` will have same effect as omitting the attribute.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_properties: Option<usize>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Object {
    /// Initialize a new [`Object`] with default [`SchemaType`]. This effectively same as calling
    /// `Object::with_type(SchemaType::Object)`.
    pub fn new() -> Self {
        Self { ..Default::default() }
    }

    /// Initialize new [`Object`] with given [`SchemaType`].
    ///
    /// Create [`std::string`] object type which can be used to define `string` field of an object.
    /// ```
    /// # use salvo_oapi::openapi::schema::{Object, SchemaType};
    /// let object = Object::with_type(SchemaType::String);
    /// ```
    pub fn with_type(schema_type: SchemaType) -> Self {
        Self {
            schema_type,
            ..Default::default()
        }
    }
    /// Add or change type of the object e.g [`SchemaType::String`].
    pub fn schema_type(mut self, schema_type: SchemaType) -> Self {
        set_value!(self schema_type schema_type)
    }

    /// Add or change additional format for detailing the schema type.
    pub fn format(mut self, format: Option<SchemaFormat>) -> Self {
        set_value!(self format format)
    }

    /// Add new property to the [`Object`].
    ///
    /// Method accepts property name and property component as an arguments.
    pub fn property<S: Into<String>, I: Into<RefOr<Schema>>>(mut self, property_name: S, component: I) -> Self {
        self.properties.insert(property_name.into(), component.into());

        self
    }

    pub fn additional_properties<I: Into<AdditionalProperties<Schema>>>(
        mut self,
        additional_properties: I,
    ) -> Self {
        set_value!(self additional_properties Some(Box::new(additional_properties.into())))
    }

    /// Add field to the required fields of [`Object`].
    pub fn required(mut self, required_field:  impl Into<String>) -> Self {
        self.required.push(Some(required_field.into()));
        self
    }

    /// Add or change the title of the [`Object`].
    pub fn title(mut self, title: impl Into<String>) -> Self {
        set_value!(self title Some(title.into()))
    }

    /// Add or change description of the property. Markdown syntax is supported.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        set_value!(self description Some(description.into()))
    }

    /// Add or change default value for the object which is provided when user has not provided the input in Swagger UI.
    pub fn default_value(mut self, default: Value) -> Self {
        set_value!(self default Some(default))
    }

    /// Add or change deprecated status for [`Object`].
    pub fn deprecated(mut self, deprecated: Deprecated) -> Self {
        set_value!(self deprecated Some(deprecated))
    }

    /// Add or change enum property variants.
    pub fn enum_values<I: IntoIterator<Item = E>, E: Into<Value>>(mut self, enum_values: I) -> Self {
        set_value!(self enum_values
                Some(enum_values.into_iter().map(|enum_value| enum_value.into()).collect()))
    }

    /// Add or change example shown in UI of the value for richer documentation.
    pub fn example(mut self, example: Option<Value>) -> Self {
        set_value!(self example example)
    }

    /// Add or change write only flag for [`Object`].
    pub fn write_only(mut self, write_only: Option<bool>) -> Self {
        set_value!(self write_only write_only)
    }

    /// Add or change read only flag for [`Object`].
    pub fn read_only(mut self, read_only: Option<bool>) -> Self {
        set_value!(self read_only read_only)
    }

    /// Add or change additional [`Xml`] formatting of the [`Object`].
    pub fn xml(mut self, xml: Option<Xml>) -> Self {
        set_value!(self xml xml)
    }

    /// Add or change nullable flag for [`Object`].
    pub fn nullable(mut self, nullable: bool) -> Self {
        set_value!(self nullable nullable)
    }

    /// Set or change _`multiple_of`_ validation flag for `number` and `integer` type values.
    pub fn multiple_of(mut self, multiple_of: Option<f64>) -> Self {
        set_value!(self multiple_of multiple_of)
    }

    /// Set or change inclusive maximum value for `number` and `integer` values.
    pub fn maximum(mut self, maximum: Option<f64>) -> Self {
        set_value!(self maximum maximum)
    }

    /// Set or change inclusive minimum value for `number` and `integer` values.
    pub fn minimum(mut self, minimum: Option<f64>) -> Self {
        set_value!(self minimum minimum)
    }

    /// Set or change exclusive maximum value for `number` and `integer` values.
    pub fn exclusive_maximum(mut self, exclusive_maximum: Option<f64>) -> Self {
        set_value!(self exclusive_maximum exclusive_maximum)
    }

    /// Set or change exclusive minimum value for `number` and `integer` values.
    pub fn exclusive_minimum(mut self, exclusive_minimum: Option<f64>) -> Self {
        set_value!(self exclusive_minimum exclusive_minimum)
    }

    /// Set or change maximum length for `string` values.
    pub fn max_length(mut self, max_length: Option<usize>) -> Self {
        set_value!(self max_length max_length)
    }

    /// Set or change minimum length for `string` values.
    pub fn min_length(mut self, min_length: Option<usize>) -> Self {
        set_value!(self min_length min_length)
    }

    /// Set or change a valid regular expression for `string` value to match.
    pub fn pattern<I: Into<String>>(mut self, pattern: Option<I>) -> Self {
        set_value!(self pattern pattern.map(|pattern| pattern.into()))
    }

    /// Set or change maximum number of properties the [`Object`] can hold.
    pub fn max_properties(mut self, max_properties: Option<usize>) -> Self {
        set_value!(self max_properties max_properties)
    }

    /// Set or change minimum number of properties the [`Object`] can hold.
    pub fn min_properties(mut self, min_properties: Option<usize>) -> Self {
        set_value!(self min_properties min_properties)
    }
}

impl From<Object> for Schema {
    fn from(s: Object) -> Self {
        Self::Object(s)
    }
}

impl ToArray for Object {}

impl From<Object> for RefOr<Schema> {
    fn from(obj: Object) -> Self {
        Self::T(Schema::Object(obj))
    }
}

/// AdditionalProperties is used to define values of map fields of the [`Schema`].
///
/// The value can either be [`RefOr`] or _`bool`_.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum AdditionalProperties<T> {
    /// Use when value type of the map is a known [`Schema`] or [`Ref`] to the [`Schema`].
    RefOr(RefOr<T>),
    /// Use _`AdditionalProperties::FreeForm(true)`_ when any value is allowed in the map.
    FreeForm(bool),
}

impl<T> From<RefOr<T>> for AdditionalProperties<T> {
    fn from(value: RefOr<T>) -> Self {
        Self::RefOr(value)
    }
}

impl From<Object> for AdditionalProperties<Schema> {
    fn from(value: Object) -> Self {
        Self::RefOr(RefOr::T(Schema::Object(value)))
    }
}

impl From<Ref> for AdditionalProperties<Schema> {
    fn from(value: Ref) -> Self {
        Self::RefOr(RefOr::Ref(value))
    }
}

/// Implements [OpenAPI Reference Object][reference] that can be used to reference
/// reusable components such as [`Schema`]s or [`Response`]s.
///
/// [reference]: https://spec.openapis.org/oas/latest.html#reference-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct Ref {
    /// Reference location of the actual component.
    #[serde(rename = "$ref")]
    pub ref_location: String,
}

impl Ref {
    /// Construct a new [`Ref`] with custom ref location. In most cases this is not necessary
    /// and [`Ref::from_schema_name`] could be used instead.
    pub fn new<I: Into<String>>(ref_location: I) -> Self {
        Self {
            ref_location: ref_location.into(),
        }
    }

    /// Construct a new [`Ref`] from provided schema name. This will create a [`Ref`] that
    /// references the the reusable schemas.
    pub fn from_schema_name<I: Into<String>>(schema_name: I) -> Self {
        Self::new(format!("#/components/schemas/{}", schema_name.into()))
    }

    /// Construct a new [`Ref`] from provided response name. This will create a [`Ref`] that
    /// references the reusable response.
    pub fn from_response_name<I: Into<String>>(response_name: I) -> Self {
        Self::new(format!("#/components/responses/{}", response_name.into()))
    }
}

impl From<Ref> for RefOr<Schema> {
    fn from(r: Ref) -> Self {
        Self::Ref(r)
    }
}


impl Default for RefOr<Schema> {
    fn default() -> Self {
        Self::T(Schema::Object(Object::new()))
    }
}

impl ToArray for RefOr<Schema> {}


/// Array represents [`Vec`] or [`slice`] type  of items.
///
/// See [`Schema::Array`] for more details.
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Array {
    /// Type will always be [`SchemaType::Array`]
    #[serde(rename = "type")]
    pub schema_type: SchemaType,

    /// Schema representing the array items type.
    pub items: Box<RefOr<Schema>>,

    /// Description of the [`Array`]. Markdown syntax is supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Marks the [`Array`] deprecated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Deprecated>,

    /// Example shown in UI of the value for richer documentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,

    /// Max length of the array.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,

    /// Min length of the array.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_items: Option<usize>,

    /// Setting this to `true` will validate successfully if all elements of this [`Array`] are
    /// unique.
    #[serde(default, skip_serializing_if = "is_false")]
    pub unique_items: bool,

    /// Xml format of the array.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xml: Option<Xml>,

    /// Set `true` to allow `"null"` to be used as value for given type.
    #[serde(default, skip_serializing_if = "is_false")]
    pub nullable: bool,
}

impl Default for Array {
    fn default() -> Self {
        Self {
            schema_type: SchemaType::Array,
            unique_items: bool::default(),
            items: Default::default(),
            description: Default::default(),
            deprecated: Default::default(),
            example: Default::default(),
            max_items: Default::default(),
            min_items: Default::default(),
            xml: Default::default(),
            nullable: Default::default(),
        }
    }
}

impl Array {
    /// Construct a new [`Array`] component from given [`Schema`].
    ///
    /// # Examples
    ///
    /// Create a `String` array component.
    /// ```
    /// # use salvo_oapi::openapi::schema::{Schema, Array, SchemaType, Object};
    /// let string_array = Array::new(Object::with_type(SchemaType::String));
    /// ```
    pub fn new<I: Into<RefOr<Schema>>>(component: I) -> Self {
        Self {
            items: Box::new(component.into()),
            ..Default::default()
        }
    }
    /// Set [`Schema`] type for the [`Array`].
    pub fn items<I: Into<RefOr<Schema>>>(mut self, component: I) -> Self {
        set_value!(self items Box::new(component.into()))
    }

    /// Add or change description of the property. Markdown syntax is supported.
    pub fn description<I: Into<String>>(mut self, description: Option<I>) -> Self {
        set_value!(self description description.map(|description| description.into()))
    }

    /// Add or change deprecated status for [`Array`].
    pub fn deprecated(mut self, deprecated: Option<Deprecated>) -> Self {
        set_value!(self deprecated deprecated)
    }

    /// Add or change example shown in UI of the value for richer documentation.
    pub fn example(mut self, example: Option<Value>) -> Self {
        set_value!(self example example)
    }

    /// Set maximum allowed length for [`Array`].
    pub fn max_items(mut self, max_items: Option<usize>) -> Self {
        set_value!(self max_items max_items)
    }

    /// Set minimum allowed length for [`Array`].
    pub fn min_items(mut self, min_items: Option<usize>) -> Self {
        set_value!(self min_items min_items)
    }

    /// Set or change whether [`Array`] should enforce all items to be unique.
    pub fn unique_items(mut self, unique_items: bool) -> Self {
        set_value!(self unique_items unique_items)
    }

    /// Set [`Xml`] formatting for [`Array`].
    pub fn xml(mut self, xml: Option<Xml>) -> Self {
        set_value!(self xml xml)
    }

    /// Add or change nullable flag for [`Object`].
    pub fn nullable(mut self, nullable: bool) -> Self {
        set_value!(self nullable nullable)
    }
}

impl From<Array> for Schema {
    fn from(array: Array) -> Self {
        Self::Array(array)
    }
}

impl From<Array> for RefOr<Schema> {
    fn from(array: Array) -> Self {
        Self::T(Schema::Array(array))
    }
}

impl ToArray for Array {}

pub trait ToArray
where
    RefOr<Schema>: From<Self>,
    Self: Sized,
{
    fn to_array(self) -> Array {
        Array::new(self)
    }
}

/// Represents data type of [`Schema`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SchemaType {
    /// Used with [`Object`]. Objects always have
    /// _schema_type_ [`SchemaType::Object`].
    Object,
    /// Indicates string type of content. Used with [`Object`] on a `string`
    /// field.
    String,
    /// Indicates integer type of content. Used with [`Object`] on a `number`
    /// field.
    Integer,
    /// Indicates floating point number type of content. Used with
    /// [`Object`] on a `number` field.
    Number,
    /// Indicates boolean type of content. Used with [`Object`] on
    /// a `bool` field.
    Boolean,
    /// Used with [`Array`]. Indicates array type of content.
    Array,
}

impl Default for SchemaType {
    fn default() -> Self {
        Self::Object
    }
}

/// Additional format for [`SchemaType`] to fine tune the data type used. If the **format** is not
/// supported by the UI it may default back to [`SchemaType`] alone.
/// Format is an open value, so you can use any formats, even not those defined by the
/// OpenAPI Specification.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase", untagged)]
pub enum SchemaFormat {
    /// Use to define additional detail about the value.
    KnownFormat(KnownFormat),
    /// Can be used to provide additional detail about the value when [`SchemaFormat::KnownFormat`]
    /// is not suitable.
    Custom(String),
}

/// Known schema format modifier property to provide fine detail of the primitive type.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KnownFormat {
    /// 8 bit integer.
    Int8,
    /// 16 bit integer.
    Int16,
    /// 32 bit integer.
    Int32,
    /// 64 bit integer.
    Int64,
    /// 8 bit unsigned integer.
    UInt8,
    /// 16 bit unsigned integer.
    UInt16,
    /// 32 bit unsigned integer.
    UInt32,
    /// 64 bit unsigned integer.
    UInt64,
    /// floating point number.
    Float,
    /// double (floating point) number.
    Double,
    /// base64 encoded chars.
    Byte,
    /// binary data (octet).
    Binary,
    /// ISO-8601 full date [FRC3339](https://xml2rfc.ietf.org/public/rfc/html/rfc3339.html#anchor14).
    Date,
    /// ISO-8601 full date time [FRC3339](https://xml2rfc.ietf.org/public/rfc/html/rfc3339.html#anchor14).
    #[serde(rename = "date-time")]
    DateTime,
    /// Hint to UI to obscure input.
    Password,
    /// Used with [`String`] values to indicate value is in UUID format.
    ///
    /// **uuid** feature need to be enabled.
    #[cfg(feature = "uuid")]
    #[cfg_attr(doc_cfg, doc(cfg(feature = "uuid")))]
    Uuid,
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::{json, Value};

    use super::*;
    use crate::openapi::*;

    #[test]
    fn create_schema_serializes_json() -> Result<(), serde_json::Error> {
        let openapi = OpenApi::new()
            .info(Info::new("My api", "1.0.0"))
            .paths(Paths::new())
            .components(
                Components::new()
                    .schema("Person", Ref::new("#/components/PersonModel"))
                    .schema(
                        "Credential",
                        Schema::from(
                            Object::new()
                                .property(
                                    "id",
                                    Object::new()
                                        .schema_type(SchemaType::Integer)
                                        .format(Some(SchemaFormat::KnownFormat(KnownFormat::Int32)))
                                        .description(Some("Id of credential"))
                                        .default(Some(json!(1i32))),
                                )
                                .property(
                                    "name",
                                    Object::new()
                                        .schema_type(SchemaType::String)
                                        .description(Some("Name of credential")),
                                )
                                .property(
                                    "status",
                                    Object::new()
                                        .schema_type(SchemaType::String)
                                        .default(Some(json!("Active")))
                                        .description(Some("Credential status"))
                                        .enum_values(Some(["Active", "NotActive", "Locked", "Expired"])),
                                )
                                .property("history", Array::new(Ref::from_schema_name("UpdateHistory")))
                                .property("tags", Object::with_type(SchemaType::String).to_array()),
                        ),
                    )
                    .build(),
            );

        let serialized = serde_json::to_string_pretty(&openapi)?;
        println!("serialized json:\n {serialized}");

        let value = serde_json::to_value(&openapi)?;
        let credential = get_json_path(&value, "components.schemas.Credential.properties");
        let person = get_json_path(&value, "components.schemas.Person");

        assert!(
            credential.get("id").is_some(),
            "could not find path: components.schemas.Credential.properties.id"
        );
        assert!(
            credential.get("status").is_some(),
            "could not find path: components.schemas.Credential.properties.status"
        );
        assert!(
            credential.get("name").is_some(),
            "could not find path: components.schemas.Credential.properties.name"
        );
        assert!(
            credential.get("history").is_some(),
            "could not find path: components.schemas.Credential.properties.history"
        );
        assert_eq!(
            credential
                .get("id")
                .unwrap_or(&serde_json::value::Value::Null)
                .to_string(),
            r#"{"default":1,"description":"Id of credential","format":"int32","type":"integer"}"#,
            "components.schemas.Credential.properties.id did not match"
        );
        assert_eq!(
            credential
                .get("name")
                .unwrap_or(&serde_json::value::Value::Null)
                .to_string(),
            r#"{"description":"Name of credential","type":"string"}"#,
            "components.schemas.Credential.properties.name did not match"
        );
        assert_eq!(
            credential
                .get("status")
                .unwrap_or(&serde_json::value::Value::Null)
                .to_string(),
            r#"{"default":"Active","description":"Credential status","enum":["Active","NotActive","Locked","Expired"],"type":"string"}"#,
            "components.schemas.Credential.properties.status did not match"
        );
        assert_eq!(
            credential
                .get("history")
                .unwrap_or(&serde_json::value::Value::Null)
                .to_string(),
            r###"{"items":{"$ref":"#/components/schemas/UpdateHistory"},"type":"array"}"###,
            "components.schemas.Credential.properties.history did not match"
        );
        assert_eq!(
            person.to_string(),
            r###"{"$ref":"#/components/PersonModel"}"###,
            "components.schemas.Person.ref did not match"
        );

        Ok(())
    }

    // Examples taken from https://spec.openapis.org/oas/latest.html#model-with-map-dictionary-properties
    #[test]
    fn test_property_order() {
        let json_value = Object::new()
            .property(
                "id",
                Object::new()
                    .schema_type(SchemaType::Integer)
                    .format(Some(SchemaFormat::KnownFormat(KnownFormat::Int32)))
                    .description(Some("Id of credential"))
                    .default(Some(json!(1i32))),
            )
            .property(
                "name",
                Object::new()
                    .schema_type(SchemaType::String)
                    .description(Some("Name of credential")),
            )
            .property(
                "status",
                Object::new()
                    .schema_type(SchemaType::String)
                    .default(Some(json!("Active")))
                    .description(Some("Credential status"))
                    .enum_values(Some(["Active", "NotActive", "Locked", "Expired"])),
            )
            .property("history", Array::new(Ref::from_schema_name("UpdateHistory")))
            .property("tags", Object::with_type(SchemaType::String).to_array())
            .build();

        #[cfg(not(feature = "preserve_order"))]
        assert_eq!(
            json_value.properties.keys().collect::<Vec<_>>(),
            vec!["history", "id", "name", "status", "tags"]
        );

        #[cfg(feature = "preserve_order")]
        assert_eq!(
            json_value.properties.keys().collect::<Vec<_>>(),
            vec!["id", "name", "status", "history", "tags"]
        );
    }

    // Examples taken from https://spec.openapis.org/oas/latest.html#model-with-map-dictionary-properties
    #[test]
    fn test_additional_properties() {
        let json_value = Object::new()
            .additional_properties(Some(Object::new().schema_type(SchemaType::String)))
            .build();
        assert_json_eq!(
            json_value,
            json!({
                "type": "object",
                "additionalProperties": {
                    "type": "string"
                }
            })
        );

        let json_value = Object::new()
            .additional_properties(Some(Ref::from_schema_name("ComplexModel")))
            .build();
        assert_json_eq!(
            json_value,
            json!({
                "type": "object",
                "additionalProperties": {
                    "$ref": "#/components/schemas/ComplexModel"
                }
            })
        )
    }

    #[test]
    fn test_object_with_title() {
        let json_value = Object::new().title(Some("SomeName")).build();
        assert_json_eq!(
            json_value,
            json!({
                "type": "object",
                "title": "SomeName"
            })
        );
    }

    #[test]
    fn derive_object_with_example() {
        let expected = r#"{"type":"object","example":{"age":20,"name":"bob the cat"}}"#;
        let json_value = Object::new()
            .example(Some(json!({"age": 20, "name": "bob the cat"})))
            .build();

        let value_string = serde_json::to_string(&json_value).unwrap();
        assert_eq!(
            value_string, expected,
            "value string != expected string, {value_string} != {expected}"
        );
    }

    fn get_json_path<'a>(value: &'a Value, path: &str) -> &'a Value {
        path.split('.').fold(value, |acc, fragment| {
            acc.get(fragment).unwrap_or(&serde_json::value::Value::Null)
        })
    }

    #[test]
    fn test_array_new() {
        let array = Array::new(
            Object::new().property(
                "id",
                Object::new()
                    .schema_type(SchemaType::Integer)
                    .format(Some(SchemaFormat::KnownFormat(KnownFormat::Int32)))
                    .description(Some("Id of credential"))
                    .default(Some(json!(1i32))),
            ),
        );

        assert!(matches!(array.schema_type, SchemaType::Array));
    }

    #[test]
    fn test_array_builder() {
        let array: Array = Array::new()
            .items(
                Object::new().property(
                    "id",
                    Object::new()
                        .schema_type(SchemaType::Integer)
                        .format(Some(SchemaFormat::KnownFormat(KnownFormat::Int32)))
                        .description(Some("Id of credential"))
                        .default(Some(json!(1i32))),
                ),
            )
            .build();

        assert!(matches!(array.schema_type, SchemaType::Array));
    }

    #[test]
    fn reserialize_deserialized_schema_components() {
        let components = Components::new()
            .schemas_from_iter(vec![(
                "Comp",
                Schema::from(
                    Object::new()
                        .property("name", Object::new().schema_type(SchemaType::String))
                        .required("name"),
                ),
            )])
            .responses_from_iter(vec![("200", Response::new().description("Okay").build())])
            .security_scheme("TLS", SecurityScheme::MutualTls { description: None })
            .build();

        let serialized_components = serde_json::to_string(&components).unwrap();

        let deserialized_components: Components = serde_json::from_str(serialized_components.as_str()).unwrap();

        assert_eq!(
            serialized_components,
            serde_json::to_string(&deserialized_components).unwrap()
        )
    }

    #[test]
    fn reserialize_deserialized_object_component() {
        let prop = Object::new()
            .property("name", Object::new().schema_type(SchemaType::String))
            .required("name")
            .build();

        let serialized_components = serde_json::to_string(&prop).unwrap();
        let deserialized_components: Object = serde_json::from_str(serialized_components.as_str()).unwrap();

        assert_eq!(
            serialized_components,
            serde_json::to_string(&deserialized_components).unwrap()
        )
    }

    #[test]
    fn reserialize_deserialized_property() {
        let prop = Object::new().schema_type(SchemaType::String).build();

        let serialized_components = serde_json::to_string(&prop).unwrap();
        let deserialized_components: Object = serde_json::from_str(serialized_components.as_str()).unwrap();

        assert_eq!(
            serialized_components,
            serde_json::to_string(&deserialized_components).unwrap()
        )
    }

    #[test]
    fn serialize_deserialize_array_within_ref_or_t_object_builder() {
        let ref_or_schema = RefOr::T(Schema::Object(
            Object::new()
                .property(
                    "test",
                    RefOr::T(Schema::Array(
                        Array::new()
                            .items(RefOr::T(Schema::Object(
                                Object::new()
                                    .property("element", RefOr::Ref(Ref::new("#/test")))
                                    .build(),
                            )))
                            .build(),
                    )),
                )
                .build(),
        ));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");

        let json_de_str = serde_json::to_string(&deserialized).expect("");
        println!("----------------------------");
        println!("{json_de_str}");

        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_one_of_within_ref_or_t_object_builder() {
        let ref_or_schema = RefOr::T(Schema::Object(
            Object::new()
                .property(
                    "test",
                    RefOr::T(Schema::OneOf(
                        OneOf::new()
                            .item(Schema::Array(
                                Array::new()
                                    .items(RefOr::T(Schema::Object(
                                        Object::new()
                                            .property("element", RefOr::Ref(Ref::new("#/test")))
                                            .build(),
                                    )))
                                    .build(),
                            ))
                            .item(Schema::Array(
                                Array::new()
                                    .items(RefOr::T(Schema::Object(
                                        Object::new()
                                            .property("foobar", RefOr::Ref(Ref::new("#/foobar")))
                                            .build(),
                                    )))
                                    .build(),
                            ))
                            .build(),
                    )),
                )
                .build(),
        ));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");

        let json_de_str = serde_json::to_string(&deserialized).expect("");
        println!("----------------------------");
        println!("{json_de_str}");

        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_all_of_of_within_ref_or_t_object_builder() {
        let ref_or_schema = RefOr::T(Schema::Object(
            Object::new()
                .property(
                    "test",
                    RefOr::T(Schema::AllOf(
                        AllOf::new()
                            .item(Schema::Array(
                                Array::new()
                                    .items(RefOr::T(Schema::Object(
                                        Object::new()
                                            .property("element", RefOr::Ref(Ref::new("#/test")))
                                            .build(),
                                    )))
                                    .build(),
                            ))
                            .item(RefOr::T(Schema::Object(
                                Object::new()
                                    .property("foobar", RefOr::Ref(Ref::new("#/foobar")))
                                    .build(),
                            )))
                            .build(),
                    )),
                )
                .build(),
        ));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");

        let json_de_str = serde_json::to_string(&deserialized).expect("");
        println!("----------------------------");
        println!("{json_de_str}");

        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_array_ref_or_t() {
        let ref_or_schema = RefOr::T(Schema::Array(
            Array::new()
                .items(RefOr::T(Schema::Object(
                    Object::new()
                        .property("element", RefOr::Ref(Ref::new("#/test")))
                        .build(),
                )))
                .build(),
        ));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");

        let json_de_str = serde_json::to_string(&deserialized).expect("");
        println!("----------------------------");
        println!("{json_de_str}");

        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_array_builder() {
        let ref_or_schema = Array::new()
            .items(RefOr::T(Schema::Object(
                Object::new()
                    .property("element", RefOr::Ref(Ref::new("#/test")))
                    .build(),
            )))
            .build();

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");

        let json_de_str = serde_json::to_string(&deserialized).expect("");
        println!("----------------------------");
        println!("{json_de_str}");

        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_with_additional_properties() {
        let schema = Schema::Object(
            Object::new()
                .property(
                    "map",
                    Object::new().additional_properties(Some(AdditionalProperties::FreeForm(true))),
                )
                .build(),
        );

        let json_str = serde_json::to_string(&schema).unwrap();
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).unwrap();

        let json_de_str = serde_json::to_string(&deserialized).unwrap();
        println!("----------------------------");
        println!("{json_de_str}");

        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_with_additional_properties_object() {
        let schema = Schema::Object(
            Object::new()
                .property(
                    "map",
                    Object::new().additional_properties(Some(
                        Object::new().property("name", Object::with_type(SchemaType::String)),
                    )),
                )
                .build(),
        );

        let json_str = serde_json::to_string(&schema).unwrap();
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).unwrap();

        let json_de_str = serde_json::to_string(&deserialized).unwrap();
        println!("----------------------------");
        println!("{json_de_str}");

        assert_eq!(json_str, json_de_str);
    }
}
