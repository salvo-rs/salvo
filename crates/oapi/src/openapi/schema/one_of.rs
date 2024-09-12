use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Discriminator, PropMap, RefOr, Schema, SchemaType};

/// OneOf [Composite Object][oneof] component holds
/// multiple components together where API endpoint could return any of them.
///
/// See [`Schema::OneOf`] for more details.
///
/// [oneof]: https://spec.openapis.org/oas/latest.html#components-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OneOf {
    /// Components of _OneOf_ component.
    #[serde(rename = "oneOf")]
    pub items: Vec<RefOr<Schema>>,

    /// Type of [`OneOf`] e.g. `SchemaType::basic(BasicType::Object)` for `object`.
    ///
    /// By default this is [`SchemaType::AnyValue`] as the type is defined by items
    /// themselves.
    #[serde(
        rename = "type",
        default = "SchemaType::any",
        skip_serializing_if = "SchemaType::is_any_value"
    )]
    pub schema_type: SchemaType,

    /// Changes the [`OneOf`] title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Description of the [`OneOf`]. Markdown syntax is supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Default value which is provided when user has not provided the input in Swagger UI.
    #[serde(rename = "default", skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,

    /// Examples shown in UI of the value for richer documentation.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub examples: Vec<Value>,

    /// Optional discriminator field can be used to aid deserialization, serialization and validation of a
    /// specific schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<Discriminator>,

    /// Optional extensions `x-something`.
    #[serde(skip_serializing_if = "PropMap::is_empty", flatten)]
    pub extensions: PropMap<String, serde_json::Value>,
}

impl Default for OneOf {
    fn default() -> Self {
        Self {
            items: Default::default(),
            schema_type: SchemaType::AnyValue,
            title: Default::default(),
            description: Default::default(),
            default_value: Default::default(),
            examples: Default::default(),
            discriminator: Default::default(),
            extensions: Default::default(),
        }
    }
}

impl OneOf {
    /// Construct a new empty [`OneOf`]. This is effectively same as calling [`OneOf::default`].
    pub fn new() -> Self {
        Default::default()
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
    /// # use salvo_oapi::schema::OneOf;
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

    /// Add or change type of the object e.g. to change type to _`string`_
    /// use value `SchemaType::Type(Type::String)`.
    pub fn schema_type<T: Into<SchemaType>>(mut self, schema_type: T) -> Self {
        self.schema_type = schema_type.into();
        self
    }

    /// Add or change the title of the [`OneOf`].
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Add or change optional description for `OneOf` component.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add or change default value for the object which is provided when user has not provided the input in Swagger UI.
    pub fn default_value(mut self, default: Value) -> Self {
        self.default_value = Some(default);
        self
    }

    /// Add or change example shown in UI of the value for richer documentation.
    pub fn add_example<V: Into<Value>>(mut self, example: V) -> Self {
        self.examples.push(example.into());
        self
    }

    /// Add or change discriminator field of the composite [`OneOf`] type.
    pub fn discriminator(mut self, discriminator: Discriminator) -> Self {
        self.discriminator = Some(discriminator);
        self
    }

    /// Add openapi extension (`x-something`) for [`OneOf`].
    pub fn add_extension<K: Into<String>>(mut self, key: K, value: serde_json::Value) -> Self {
        self.extensions.insert(key.into(), value);
        self
    }
}

impl From<OneOf> for Schema {
    fn from(one_of: OneOf) -> Self {
        Self::OneOf(one_of)
    }
}

impl From<OneOf> for RefOr<Schema> {
    fn from(one_of: OneOf) -> Self {
        Self::Type(Schema::OneOf(one_of))
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_build_one_of() {
        let one_of = OneOf::with_capacity(5)
            .title("title")
            .description("description")
            .default_value(Value::String("default".to_string()))
            .add_example(Value::String("example1".to_string()))
            .add_example(Value::String("example2".to_string()))
            .discriminator(Discriminator::new("discriminator".to_string()));

        assert_eq!(one_of.items.len(), 0);
        assert_eq!(one_of.items.capacity(), 5);
        assert_json_eq!(
            one_of,
            json!({
                "oneOf": [],
                "title": "title",
                "description": "description",
                "default": "default",
                "examples": ["example1", "example2"],
                "discriminator": {
                    "propertyName": "discriminator"
                }
            })
        )
    }

    #[test]
    fn test_schema_from_one_of() {
        let one_of = OneOf::new();
        let schema = Schema::from(one_of);
        assert_json_eq!(
            schema,
            json!({
                "oneOf": []
            })
        )
    }

    #[test]
    fn test_refor_schema_from_one_of() {
        let one_of = OneOf::new();
        let ref_or: RefOr<Schema> = RefOr::from(one_of);
        assert_json_eq!(
            ref_or,
            json!({
                "oneOf": []
            })
        )
    }

    #[test]
    fn test_oneof_with_extensions() {
        let expected = json!("value");
        let json_value = OneOf::new().add_extension("x-some-extension", expected.clone());

        let value = serde_json::to_value(&json_value).unwrap();
        assert_eq!(value.get("x-some-extension"), Some(&expected));
    }
}
