use serde::de::Visitor;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schema::{AllOf, AnyOf, BasicType, Object, OneOf, Ref};
use crate::{Deprecated, PropMap, RefOr, Schema, SchemaType, Xml};

/// Represents [`Array`] items in [JSON Schema Array][json_schema_array].
///
/// [json_schema_array]: <https://json-schema.org/understanding-json-schema/reference/array#items>
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum ArrayItems {
    /// Defines [`Array::items`] as [`RefOr::T(Schema)`]. This is the default for [`Array`].
    RefOrSchema(Box<RefOr<Schema>>),
    /// Defines [`Array::items`] as `false` indicating that no extra items are allowed to the
    /// [`Array`]. This can be used together with [`Array::prefix_items`] to disallow [additional
    /// items][additional_items] in [`Array`].
    ///
    /// [additional_items]: <https://json-schema.org/understanding-json-schema/reference/array#additionalitems>
    #[serde(with = "array_items_false")]
    False,
}

impl Default for ArrayItems {
    fn default() -> Self {
        Self::RefOrSchema(Box::new(Object::with_type(BasicType::Object).into()))
    }
}

impl From<RefOr<Schema>> for ArrayItems {
    fn from(value: RefOr<Schema>) -> Self {
        Self::RefOrSchema(Box::new(value))
    }
}

impl From<Schema> for ArrayItems {
    fn from(value: Schema) -> Self {
        Self::RefOrSchema(Box::new(RefOr::Type(value)))
    }
}

impl From<Object> for ArrayItems {
    fn from(value: Object) -> Self {
        Self::RefOrSchema(Box::new(value.into()))
    }
}

impl From<Ref> for ArrayItems {
    fn from(value: Ref) -> Self {
        Self::RefOrSchema(Box::new(value.into()))
    }
}

impl From<AllOf> for ArrayItems {
    fn from(value: AllOf) -> Self {
        Self::RefOrSchema(Box::new(value.into()))
    }
}

impl From<AnyOf> for ArrayItems {
    fn from(value: AnyOf) -> Self {
        Self::RefOrSchema(Box::new(value.into()))
    }
}

impl From<OneOf> for ArrayItems {
    fn from(value: OneOf) -> Self {
        Self::RefOrSchema(Box::new(value.into()))
    }
}

impl From<Array> for ArrayItems {
    fn from(value: Array) -> Self {
        Self::RefOrSchema(Box::new(value.into()))
    }
}

mod array_items_false {
    use super::*;

    pub(super) fn serialize<S: serde::Serializer>(serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bool(false)
    }

    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<(), D::Error> {
        struct ItemsFalseVisitor;

        impl<'de> Visitor<'de> for ItemsFalseVisitor {
            type Value = ();
            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if !v {
                    Ok(())
                } else {
                    Err(serde::de::Error::custom(format!(
                        "invalid boolean value: {v}, expected false"
                    )))
                }
            }

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("expected boolean false")
            }
        }

        deserializer.deserialize_bool(ItemsFalseVisitor)
    }
}

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
    pub items: ArrayItems,

    /// Prefix items of [`Array`] is used to define item validation of tuples according to
    /// [JSON schema item validation][item_validation].
    ///
    /// [item_validation]: <https://json-schema.org/understanding-json-schema/reference/array#tupleValidation>
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub prefix_items: Vec<RefOr<Schema>>,

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
            prefix_items: Vec::default(),
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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Set [`Schema`] type for the [`Array`].
    #[must_use]
    pub fn items<I: Into<ArrayItems>>(mut self, items: I) -> Self {
        self.items = items.into();
        self
    }

    /// Add prefix items of [`Array`] to define item validation of tuples according to
    /// [JSON schema item validation][item_validation].
    ///
    /// [item_validation]: <https://json-schema.org/understanding-json-schema/reference/array#tupleValidation>
    #[must_use]
    pub fn prefix_items<I: IntoIterator<Item = S>, S: Into<RefOr<Schema>>>(
        mut self,
        items: I,
    ) -> Self {
        self.prefix_items = items
            .into_iter()
            .map(|item| item.into())
            .collect::<Vec<_>>();
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
    #[must_use]
    pub fn schema_type<T: Into<SchemaType>>(mut self, schema_type: T) -> Self {
        self.schema_type = schema_type.into();
        self
    }

    /// Add or change the title of the [`Array`].
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Add or change description of the property. Markdown syntax is supported.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add or change deprecated status for [`Array`].
    #[must_use]
    pub fn deprecated(mut self, deprecated: Deprecated) -> Self {
        self.deprecated = Some(deprecated);
        self
    }

    /// Add or change example shown in UI of the value for richer documentation.
    #[must_use]
    pub fn example<V: Into<Value>>(mut self, example: V) -> Self {
        self.examples.push(example.into());
        self
    }

    /// Add or change example shown in UI of the value for richer documentation.
    #[must_use]
    pub fn examples<I: IntoIterator<Item = V>, V: Into<Value>>(mut self, examples: I) -> Self {
        self.examples = examples.into_iter().map(Into::into).collect();
        self
    }

    /// Add or change default value for the object which is provided when user has not provided the
    /// input in Swagger UI.
    #[must_use]
    pub fn default_value(mut self, default: Value) -> Self {
        self.default_value = Some(default);
        self
    }

    /// Set maximum allowed length for [`Array`].
    #[must_use]
    pub fn max_items(mut self, max_items: usize) -> Self {
        self.max_items = Some(max_items);
        self
    }

    /// Set minimum allowed length for [`Array`].
    #[must_use]
    pub fn min_items(mut self, min_items: usize) -> Self {
        self.min_items = Some(min_items);
        self
    }

    /// Set or change whether [`Array`] should enforce all items to be unique.
    #[must_use]
    pub fn unique_items(mut self, unique_items: bool) -> Self {
        self.unique_items = unique_items;
        self
    }

    /// Set [`Xml`] formatting for [`Array`].
    #[must_use]
    pub fn xml(mut self, xml: Xml) -> Self {
        self.xml = Some(xml);
        self
    }

    /// Add openapi extension (`x-something`) for [`Array`].
    #[must_use]
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

// impl ToArray for Array {}

// /// Trait for converting a type to [`Array`].
// pub trait ToArray
// where
//     RefOr<Schema>: From<Self>,
//     Self: Sized,
// {
//     /// Convert a type to [`Array`].
//     pub fn to_array(self) -> Array {
//         Array::new().items(self)
//     }
// }

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
                Value::String("example1".to_owned()),
                Value::String("example2".to_owned()),
            ])
            .default_value(Value::String("default".to_owned()))
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

    #[test]
    fn test_array_with_prefix_items() {
        let array = Array::new().items(ArrayItems::False).prefix_items([
            Object::with_type(BasicType::String),
            Object::with_type(BasicType::Number),
        ]);

        assert_json_eq!(
            array,
            json!({
                "type": "array",
                "items": false,
                "prefixItems": [
                    { "type": "string" },
                    { "type": "number" }
                ]
            })
        )
    }

    #[test]
    fn test_array_items_false_deserialize() {
        let json = json!({
            "type": "array",
            "items": false,
            "prefixItems": [
                { "type": "string" }
            ]
        });
        let array: Array = serde_json::from_value(json).unwrap();
        assert_eq!(array.items, ArrayItems::False);
        assert_eq!(array.prefix_items.len(), 1);
    }
}
