use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schema::BasicType;
use crate::{Deprecated, PropMap, RefOr, Schema, SchemaType, Xml};

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

    /// Examples shown in UI of the value for richer documentation.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub examples: Vec<Value>,

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

    /// Optional extensions `x-something`.
    #[serde(skip_serializing_if = "PropMap::is_empty", flatten)]
    pub extensions: PropMap<String, serde_json::Value>,
}

impl Default for Array {
    fn default() -> Self {
        Self {
            title: Default::default(),
            schema_type: BasicType::Array.into(),
            unique_items: bool::default(),
            items: Default::default(),
            description: Default::default(),
            deprecated: Default::default(),
            examples: Default::default(),
            default_value: Default::default(),
            max_items: Default::default(),
            min_items: Default::default(),
            xml: Default::default(),
            extensions: Default::default(),
        }
    }
}

impl Array {
    /// Construct a new [`Array`] component from given [`Schema`].
    ///
    /// # Examples
    ///
    /// _**Create a `String` array component.**_
    /// ```
    /// # use salvo_oapi::schema::{Schema, Array, SchemaType, BasicType, Object};
    /// let string_array = Array::new().items(Object::with_type(BasicType::String));
    /// ```
    pub fn new() -> Self {
        Self::default()
    }
    /// Set [`Schema`] type for the [`Array`].
    pub fn items<I: Into<RefOr<Schema>>>(mut self, items: I) -> Self {
        self.items = Box::new(items.into());
        self
    }

    /// Change type of the array e.g. to change type to _`string`_
    /// use value `SchemaType::Type(Type::String)`.
    ///
    /// # Examples
    ///
    /// _**Make nullable string array.**_
    /// ```rust
    /// # use salvo_oapi::schema::{Array, BasicType, SchemaType, Object};
    /// let _ = Array::new()
    ///     .schema_type(SchemaType::from_iter([BasicType::Array, BasicType::Null]))
    ///     .items(Object::with_type(BasicType::String));
    /// ```
    pub fn schema_type<T: Into<SchemaType>>(mut self, schema_type: T) -> Self {
        self.schema_type = schema_type.into();
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
    pub fn example<V: Into<Value>>(mut self, example: V) -> Self {
        self.examples.push(example.into());
        self
    }

    /// Add or change example shown in UI of the value for richer documentation.
    pub fn examples<I: IntoIterator<Item = V>, V: Into<Value>>(mut self, examples: I) -> Self {
        self.examples = examples.into_iter().map(Into::into).collect();
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

    /// Add openapi extension (`x-something`) for [`Array`].
    pub fn add_extension<K: Into<String>>(mut self, key: K, value: serde_json::Value) -> Self {
        self.extensions.insert(key.into(), value);
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
        Self::Type(Schema::Array(array))
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
        Array::new().items(self)
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
        let array = Array::new()
            .items(Object::with_type(BasicType::String))
            .title("title")
            .description("description")
            .deprecated(Deprecated::False)
            .examples([
                Value::String("example1".to_string()),
                Value::String("example2".to_string()),
            ])
            .default_value(Value::String("default".to_string()))
            .max_items(10)
            .min_items(1)
            .unique_items(true)
            .xml(Xml::new());

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
                "examples": ["example1", "example2"],
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

    #[test]
    fn test_array_with_extensions() {
        let expected = json!("value");
        let json_value = Array::default().add_extension("x-some-extension", expected.clone());

        let value = serde_json::to_value(&json_value).unwrap();
        assert_eq!(value.get("x-some-extension"), Some(&expected));
    }
}
