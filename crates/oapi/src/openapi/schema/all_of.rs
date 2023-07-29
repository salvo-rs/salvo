use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Discriminator, RefOr, Schema};

/// AllOf [Composite Object][allof] component holds
/// multiple components together where API endpoint will return a combination of all of them.
///
/// See [`Schema::AllOf`] for more details.
///
/// [allof]: https://spec.openapis.org/oas/latest.html#components-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct AllOf {
    /// Components of _AllOf_ component.
    #[serde(rename = "allOf")]
    pub items: Vec<RefOr<Schema>>,

    /// Changes the [`AllOf`] title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

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
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub nullable: bool,
}

impl AllOf {
    /// Construct a new empty [`AllOf`]. This is effectively same as calling [`AllOf::default`].
    pub fn new() -> Self {
        Default::default()
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
    /// # use salvo_oapi::schema::AllOf;
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

    /// Add or change the title of the [`AllOf`].
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Add or change optional description for `AllOf` component.
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

    /// Add or change discriminator field of the composite [`AllOf`] type.
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
