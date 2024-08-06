use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Discriminator, PropMap, RefOr, Schema};

/// OneOf [Composite Object][oneof] component holds
/// multiple components together where API endpoint could return any of them.
///
/// See [`Schema::OneOf`] for more details.
///
/// [oneof]: https://spec.openapis.org/oas/latest.html#components-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct OneOf {
    /// Components of _OneOf_ component.
    #[serde(rename = "oneOf")]
    pub items: Vec<RefOr<Schema>>,

    /// Changes the [`OneOf`] title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Description of the [`OneOf`]. Markdown syntax is supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Default value which is provided when user has not provided the input in Swagger UI.
    #[serde(rename = "default", skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,

    /// Example shown in UI of the value for richer documentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,

    /// Optional discriminator field can be used to aid deserialization, serialization and validation of a
    /// specific schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<Discriminator>,

    /// Set `true` to allow `"null"` to be used as value for given type.
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub nullable: bool,

    /// Optional extensions `x-something`.
    #[serde(skip_serializing_if = "Option::is_none", flatten)]
    pub extensions: Option<PropMap<String, serde_json::Value>>,
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
    pub fn example(mut self, example: Value) -> Self {
        self.example = Some(example);
        self
    }

    /// Add or change discriminator field of the composite [`OneOf`] type.
    pub fn discriminator(mut self, discriminator: Discriminator) -> Self {
        self.discriminator = Some(discriminator);
        self
    }

    /// Add or change nullable flag for [Object][crate::Object].
    pub fn nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    /// Add openapi extensions (`x-something`) for [`OneOf`].
    pub fn extensions(mut self, extensions: Option<PropMap<String, serde_json::Value>>) -> Self {
        self.extensions = extensions;
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
        Self::T(Schema::OneOf(one_of))
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
            .example(Value::String("example".to_string()))
            .discriminator(Discriminator::new("discriminator".to_string()))
            .nullable(true);

        assert_eq!(one_of.items.len(), 0);
        assert_eq!(one_of.items.capacity(), 5);
        assert_json_eq!(
            one_of,
            json!({
                "oneOf": [],
                "title": "title",
                "description": "description",
                "default": "default",
                "example": "example",
                "discriminator": {
                    "propertyName": "discriminator"
                },
                "nullable": true
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
        let json_value = OneOf::new().extensions(Some([("x-some-extension".to_string(), expected.clone())].into()));

        let value = serde_json::to_value(&json_value).unwrap();
        assert_eq!(value.get("x-some-extension"), Some(&expected));
    }
}
