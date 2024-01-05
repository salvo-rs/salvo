use std::collections::BTreeMap;

use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::AdditionalProperties;
use crate::{Deprecated, RefOr, Schema, SchemaFormat, SchemaType, ToArray, Xml};

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
    // #[doc(hidden)]
    // #[serde(skip)]
    // pub origin_type_id: Option<TypeId>,
    /// Type of [`Object`] e.g. [`SchemaType::Object`] for `object` and [`SchemaType::String`] for
    /// `string` types.
    #[serde(rename = "type")]
    pub schema_type: SchemaType,

    /// Changes the [`Object`] symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,

    /// Additional format for detailing the schema type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<SchemaFormat>,

    /// Description of the [`Object`]. Markdown syntax is supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Default value which is provided when user has not provided the input in Swagger UI.
    #[serde(rename = "default", skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,

    /// Enum variants of fields that can be represented as `unit` type `enums`
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<Value>>,

    /// Vector of required field names.
    #[serde(skip_serializing_if = "IndexSet::is_empty", default = "IndexSet::new")]
    pub required: IndexSet<String>,

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
    #[serde(default, skip_serializing_if = "super::is_false")]
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

impl Object {
    /// Initialize a new [`Object`] with default [`SchemaType`]. This effectively same as calling
    /// `Object::with_type(SchemaType::Object)`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Initialize new [`Object`] with given [`SchemaType`].
    ///
    /// Create [`std::string`] object type which can be used to define `string` field of an object.
    /// ```
    /// # use salvo_oapi::schema::{Object, SchemaType};
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
        self.schema_type = schema_type;
        self
    }

    /// Add or change additional format for detailing the schema type.
    pub fn format(mut self, format: SchemaFormat) -> Self {
        self.format = Some(format);
        self
    }

    /// Add new property to the [`Object`].
    ///
    /// Method accepts property name and property component as an arguments.
    pub fn property<S: Into<String>, I: Into<RefOr<Schema>>>(mut self, property_name: S, component: I) -> Self {
        self.properties.insert(property_name.into(), component.into());

        self
    }

    /// Add additional properties to the [`Object`].
    pub fn additional_properties<I: Into<AdditionalProperties<Schema>>>(mut self, additional_properties: I) -> Self {
        self.additional_properties = Some(Box::new(additional_properties.into()));
        self
    }

    /// Add field to the required fields of [`Object`].
    pub fn required(mut self, required_field: impl Into<String>) -> Self {
        self.required.insert(required_field.into());
        self
    }

    /// Add or change the symbol of the [`Object`].
    pub fn symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }

    /// Add or change description of the property. Markdown syntax is supported.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add or change default value for the object which is provided when user has not provided the input in Swagger UI.
    pub fn default_value(mut self, default: Value) -> Self {
        self.default_value = Some(default);
        self
    }

    /// Add or change deprecated status for [`Object`].
    pub fn deprecated(mut self, deprecated: Deprecated) -> Self {
        self.deprecated = Some(deprecated);
        self
    }

    /// Add or change enum property variants.
    pub fn enum_values<I, E>(mut self, enum_values: I) -> Self
    where
        I: IntoIterator<Item = E>,
        E: Into<Value>,
    {
        self.enum_values = Some(enum_values.into_iter().map(|enum_value| enum_value.into()).collect());
        self
    }

    /// Add or change example shown in UI of the value for richer documentation.
    pub fn example(mut self, example: Value) -> Self {
        self.example = Some(example);
        self
    }

    /// Add or change write only flag for [`Object`].
    pub fn write_only(mut self, write_only: bool) -> Self {
        self.write_only = Some(write_only);
        self
    }

    /// Add or change read only flag for [`Object`].
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = Some(read_only);
        self
    }

    /// Add or change additional [`Xml`] formatting of the [`Object`].
    pub fn xml(mut self, xml: Xml) -> Self {
        self.xml = Some(xml);
        self
    }

    /// Add or change nullable flag for [`Object`].
    pub fn nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    /// Set or change _`multiple_of`_ validation flag for `number` and `integer` type values.
    pub fn multiple_of(mut self, multiple_of: f64) -> Self {
        self.multiple_of = Some(multiple_of);
        self
    }

    /// Set or change inclusive maximum value for `number` and `integer` values.
    pub fn maximum(mut self, maximum: f64) -> Self {
        self.maximum = Some(maximum);
        self
    }

    /// Set or change inclusive minimum value for `number` and `integer` values.
    pub fn minimum(mut self, minimum: f64) -> Self {
        self.minimum = Some(minimum);
        self
    }

    /// Set or change exclusive maximum value for `number` and `integer` values.
    pub fn exclusive_maximum(mut self, exclusive_maximum: f64) -> Self {
        self.exclusive_maximum = Some(exclusive_maximum);
        self
    }

    /// Set or change exclusive minimum value for `number` and `integer` values.
    pub fn exclusive_minimum(mut self, exclusive_minimum: f64) -> Self {
        self.exclusive_minimum = Some(exclusive_minimum);
        self
    }

    /// Set or change maximum length for `string` values.
    pub fn max_length(mut self, max_length: usize) -> Self {
        self.max_length = Some(max_length);
        self
    }

    /// Set or change minimum length for `string` values.
    pub fn min_length(mut self, min_length: usize) -> Self {
        self.min_length = Some(min_length);
        self
    }

    /// Set or change a valid regular expression for `string` value to match.
    pub fn pattern<I: Into<String>>(mut self, pattern: I) -> Self {
        self.pattern = Some(pattern.into());
        self
    }

    /// Set or change maximum number of properties the [`Object`] can hold.
    pub fn max_properties(mut self, max_properties: usize) -> Self {
        self.max_properties = Some(max_properties);
        self
    }

    /// Set or change minimum number of properties the [`Object`] can hold.
    pub fn min_properties(mut self, min_properties: usize) -> Self {
        self.min_properties = Some(min_properties);
        self
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

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_build_string_object() {
        let object = Object::new()
            .schema_type(SchemaType::String)
            .deprecated(Deprecated::True)
            .write_only(false)
            .read_only(true)
            .xml(Xml::new())
            .max_length(10)
            .min_length(1)
            .pattern(r"^[a-z]+$");

        assert_json_eq!(
            object,
            json!({
                "type": "string",
                "deprecated": true,
                "readOnly": true,
                "writeOnly": false,
                "xml": {},
                "minLength": 1,
                "maxLength": 10,
                "pattern": "^[a-z]+$"
            })
        );
    }

    #[test]
    fn test_build_number_object() {
        let object = Object::new()
            .schema_type(SchemaType::Number)
            .deprecated(Deprecated::True)
            .write_only(false)
            .read_only(true)
            .xml(Xml::new())
            .multiple_of(10.0)
            .minimum(0.0)
            .maximum(1000.0)
            .exclusive_minimum(0.0)
            .exclusive_maximum(1000.0);

        assert_json_eq!(
            object,
            json!({
                "type": "number",
                "deprecated": true,
                "readOnly": true,
                "writeOnly": false,
                "xml": {},
                "multipleOf": 10.0,
                "minimum": 0.0,
                "maximum": 1000.0,
                "exclusiveMinimum": 0.0,
                "exclusiveMaximum": 1000.0
            })
        );
    }

    #[test]
    fn test_build_object_object() {
        let object = Object::new()
            .schema_type(SchemaType::Object)
            .deprecated(Deprecated::True)
            .write_only(false)
            .read_only(true)
            .xml(Xml::new())
            .min_properties(1)
            .max_properties(10);

        assert_json_eq!(
            object,
            json!({
                "type": "object",
                "deprecated": true,
                "readOnly": true,
                "writeOnly": false,
                "xml": {},
                "minProperties": 1,
                "maxProperties": 10
            })
        );
    }
}
