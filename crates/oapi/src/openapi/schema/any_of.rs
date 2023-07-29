use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Discriminator, RefOr, Schema};

/// AnyOf [Composite Object][allof] component holds
/// multiple components together where API endpoint will return a combination of all of them.
///
/// See [`Schema::AnyOf`] for more details.
///
/// [allof]: https://spec.openapis.org/oas/latest.html#components-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct AnyOf {
    /// Components of _AnyOf_ component.
    #[serde(rename = "anyOf")]
    pub items: Vec<RefOr<Schema>>,

    /// Changes the [`AnyOf`] title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Description of the [`AnyOf`]. Markdown syntax is supported.
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
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub nullable: bool,
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
        self.default = Some(default);
        self
    }

    /// Add or change example shown in UI of the value for richer documentation.
    pub fn example(mut self, example: Value) -> Self {
        self.example = Some(example);
        self
    }

    /// Add or change discriminator field of the composite [`AnyOf`] type.
    pub fn discriminator(mut self, discriminator: Discriminator) -> Self {
        self.discriminator = Some(discriminator);
        self
    }

    /// Add or change nullable flag for [`Object`].
    pub fn nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
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
