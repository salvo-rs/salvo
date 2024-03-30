use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Deprecated, RefOr, Schema, SchemaType, Xml};

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

    /// Changes the [`Array`] title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

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

    /// Default value which is provided when user has not provided the input in Swagger UI.
    #[serde(rename = "default", skip_serializing_if = "Option::is_none")]
    pub default_value: Option<Value>,

    /// Max length of the array.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,

    /// Min length of the array.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_items: Option<usize>,

    /// Setting this to `true` will validate successfully if all elements of this [`Array`] are
    /// unique.
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub unique_items: bool,

    /// Xml format of the array.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xml: Option<Xml>,

    /// Set `true` to allow `"null"` to be used as value for given type.
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub nullable: bool,
}

impl Default for Array {
    fn default() -> Self {
        Self {
            title: Default::default(),
            schema_type: SchemaType::Array,
            unique_items: bool::default(),
            items: Default::default(),
            description: Default::default(),
            deprecated: Default::default(),
            example: Default::default(),
            default_value: Default::default(),
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
    /// # use salvo_oapi::schema::{Schema, Array, SchemaType, Object};
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
        self.items = Box::new(component.into());
        self
    }

    /// Add or change the title of the [`Array`].
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Add or change description of the property. Markdown syntax is supported.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add or change deprecated status for [`Array`].
    pub fn deprecated(mut self, deprecated: Deprecated) -> Self {
        self.deprecated = Some(deprecated);
        self
    }

    /// Add or change example shown in UI of the value for richer documentation.
    pub fn example(mut self, example: Value) -> Self {
        self.example = Some(example);
        self
    }

    /// Add or change default value for the object which is provided when user has not provided the input in Swagger UI.
    pub fn default_value(mut self, default: Value) -> Self {
        self.default_value = Some(default);
        self
    }

    /// Set maximum allowed length for [`Array`].
    pub fn max_items(mut self, max_items: usize) -> Self {
        self.max_items = Some(max_items);
        self
    }

    /// Set minimum allowed length for [`Array`].
    pub fn min_items(mut self, min_items: usize) -> Self {
        self.min_items = Some(min_items);
        self
    }

    /// Set or change whether [`Array`] should enforce all items to be unique.
    pub fn unique_items(mut self, unique_items: bool) -> Self {
        self.unique_items = unique_items;
        self
    }

    /// Set [`Xml`] formatting for [`Array`].
    pub fn xml(mut self, xml: Xml) -> Self {
        self.xml = Some(xml);
        self
    }

    /// Add or change nullable flag for [Object][crate::Object].
    pub fn nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
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

/// Trait for converting a type to [`Array`].
pub trait ToArray
where
    RefOr<Schema>: From<Self>,
    Self: Sized,
{
    /// Convert a type to [`Array`].
    fn to_array(self) -> Array {
        Array::new(self)
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;
    use crate::Object;

    #[test]
    fn test_build_array() {
        let array = Array::new(Object::with_type(SchemaType::Object))
            .items(Object::with_type(SchemaType::String))
            .title("title")
            .description("description")
            .deprecated(Deprecated::False)
            .example(Value::String("example".to_string()))
            .default_value(Value::String("default".to_string()))
            .max_items(10)
            .min_items(1)
            .unique_items(true)
            .xml(Xml::new())
            .nullable(false);

        assert_json_eq!(
            array,
            json!({
                "type": "array",
                "items": {
                    "type": "string"
                },
                "title": "title",
                "description": "description",
                "deprecated": false,
                "example": "example",
                "default": "default",
                "maxItems": 10,
                "minItems": 1,
                "uniqueItems": true,
                "xml": {},
            })
        )
    }

    #[test]
    fn test_schema_from_array() {
        let array = Array::default();
        let schema = Schema::from(array);
        assert_json_eq!(
            schema,
            json!({
                "type": "array",
                "items": {
                    "type": "object"
                }
            })
        )
    }
}
