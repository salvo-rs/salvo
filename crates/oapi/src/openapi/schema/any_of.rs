use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::SchemaType;
use crate::{Discriminator, PropMap, RefOr, Schema};

/// AnyOf [Composite Object][allof] component holds
/// multiple components together where API endpoint will return a combination of all of them.
///
/// See [`Schema::AnyOf`] for more details.
///
/// [allof]: https://spec.openapis.org/oas/latest.html#components-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AnyOf {
    /// Components of _AnyOf_ component.
    #[serde(rename = "anyOf")]
    pub items: Vec<RefOr<Schema>>,

    /// Type of [`AnyOf`] e.g. `SchemaType::basic(BasicType::Object)` for `object`.
    ///
    /// By default this is [`SchemaType::AnyValue`] as the type is defined by items
    /// themselves.
    #[serde(
        rename = "type",
        default = "SchemaType::any",
        skip_serializing_if = "SchemaType::is_any_value"
    )]
    pub schema_type: SchemaType,

    /// Changes the [`AnyOf`] title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Description of the [`AnyOf`]. Markdown syntax is supported.
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
    #[serde(skip_serializing_if = "Option::is_none", flatten)]
    pub extensions: Option<PropMap<String, serde_json::Value>>,
}

impl Default for AnyOf {
    fn default() -> Self {
        Self {
            items: Default::default(),
            schema_type: SchemaType::AnyValue,
            title: None,
            description: Default::default(),
            default_value: Default::default(),
            examples: Default::default(),
            discriminator: Default::default(),
            extensions: Default::default(),
        }
    }
}

impl AnyOf {
    /// Construct a new empty [`AnyOf`]. This is effectively same as calling [`AnyOf::default`].
    pub fn new() -> Self {
        Default::default()
    }

    /// Construct a new [`AnyOf`] component with given capacity.
    ///
    /// AnyOf component is then able to contain number of components without
    /// reallocating.
    ///
    /// # Examples
    ///
    /// Create [`AnyOf`] component with initial capacity of 5.
    /// ```
    /// # use salvo_oapi::schema::AnyOf;
    /// let one_of = AnyOf::with_capacity(5);
    /// ```
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
            ..Default::default()
        }
    }
    /// Adds a given [`Schema`] to [`AnyOf`] [Composite Object][composite]
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

    /// Add or change the title of the [`AnyOf`].
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Add or change optional description for `AnyOf` component.
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
    pub fn example<V: Into<Value>>(mut self, example: V) -> Self {
        self.examples.push(example.into());
        self
    }

    /// Add or change examples shown in UI of the value for richer documentation.
    pub fn examples<I: IntoIterator<Item = V>, V: Into<Value>>(mut self, examples: I) -> Self {
        self.examples = examples.into_iter().map(Into::into).collect();
        self
    }

    /// Add or change discriminator field of the composite [`AnyOf`] type.
    pub fn discriminator(mut self, discriminator: Discriminator) -> Self {
        self.discriminator = Some(discriminator);
        self
    }

    /// Add openapi extensions (`x-something`) for [`AnyOf`].
    pub fn extensions(mut self, extensions: Option<PropMap<String, serde_json::Value>>) -> Self {
        self.extensions = extensions;
        self
    }
}

impl From<AnyOf> for Schema {
    fn from(one_of: AnyOf) -> Self {
        Self::AnyOf(one_of)
    }
}

impl From<AnyOf> for RefOr<Schema> {
    fn from(one_of: AnyOf) -> Self {
        Self::T(Schema::AnyOf(one_of))
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_build_any_of() {
        let any_of = AnyOf::with_capacity(5)
            .title("title")
            .description("description")
            .default_value(Value::String("default".to_string()))
            .examples([
                Value::String("example1".to_string()),
                Value::String("example2".to_string()),
            ])
            .discriminator(Discriminator::new("discriminator".to_string()));

        assert_eq!(any_of.items.len(), 0);
        assert_eq!(any_of.items.capacity(), 5);
        assert_json_eq!(
            any_of,
            json!({
                "anyOf": [],
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
    fn test_schema_from_any_of() {
        let any_of = AnyOf::new();
        let schema = Schema::from(any_of);
        assert_json_eq!(
            schema,
            json!({
                "anyOf": []
            })
        )
    }

    #[test]
    fn test_refor_schema_from_any_of() {
        let any_of = AnyOf::new();
        let ref_or: RefOr<Schema> = RefOr::from(any_of);
        assert_json_eq!(
            ref_or,
            json!({
                "anyOf": []
            })
        )
    }

    #[test]
    fn test_anyof_with_extensions() {
        let expected = json!("value");
        let json_value = AnyOf::new().extensions(Some([("x-some-extension".to_string(), expected.clone())].into()));

        let value = serde_json::to_value(&json_value).unwrap();
        assert_eq!(value.get("x-some-extension"), Some(&expected));
    }
}
