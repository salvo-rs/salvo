//! Implements content object for request body and response.
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::encoding::Encoding;
use super::example::Example;
use super::{PropMap, RefOr, Schema};

/// Content holds request body content or response content.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Content {
    /// Reference to a reusable Media Type Object, e.g. one defined under
    /// `components.mediaTypes`. Added in OpenAPI 3.2.
    ///
    /// `content` maps are typed as [`Content`] rather than `RefOr<Content>` so that existing
    /// callers keep compiling; a [`Content`] carrying only this field serializes exactly as a
    /// Reference Object. As with any Reference Object, sibling fields are ignored by consumers.
    ///
    /// ```
    /// # use salvo_oapi::Content;
    /// let content = Content::from_ref("#/components/mediaTypes/FramePayload");
    /// ```
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none", default)]
    pub ref_location: Option<String>,

    /// Schema used in response body or request body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<RefOr<Schema>>,

    /// Schema describing each item within a sequential media type, e.g. `text/event-stream` or
    /// `application/jsonl`. Added in OpenAPI 3.2.
    ///
    /// Unlike [`Content::schema`], which applies to the complete content, `item_schema` applies
    /// to each item in the stream independently. Both may be used together.
    ///
    /// See <https://spec.openapis.org/oas/v3.2.0.html#media-type-object>.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub item_schema: Option<RefOr<Schema>>,

    /// Example for request body or response body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,

    /// Examples of the request body or response body. [`Content::examples`] should match to
    /// media type and specified schema if present. [`Content::examples`] and
    /// [`Content::example`] are mutually exclusive. If both are defined `examples` will
    /// override value in `example`.
    #[serde(default, skip_serializing_if = "PropMap::is_empty")]
    pub examples: PropMap<String, RefOr<Example>>,

    /// A map between a property name and its encoding information.
    ///
    /// The key, being the property name, MUST exist in the [`Content::schema`] as a property, with
    /// `schema` being a [`Schema::Object`] and this object containing the same property key in
    /// [`Object::properties`](crate::schema::Object::properties).
    ///
    /// The encoding object SHALL only apply to `request_body` objects when the media type is
    /// multipart or `application/x-www-form-urlencoded`.
    ///
    /// Must not be combined with [`Content::prefix_encoding`] or [`Content::item_encoding`].
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub encoding: PropMap<String, Encoding>,

    /// Positional encoding information, applied to the array item at the same index. Added in
    /// OpenAPI 3.2 and only applicable to `multipart` media types.
    ///
    /// Requires either [`Content::item_schema`] or an array [`Content::schema`] to be present,
    /// and must not be combined with [`Content::encoding`].
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub prefix_encoding: Vec<Encoding>,

    /// A single encoding applied to all array items not covered by
    /// [`Content::prefix_encoding`]. Added in OpenAPI 3.2 and only applicable to `multipart`
    /// media types.
    ///
    /// Requires either [`Content::item_schema`] or an array [`Content::schema`] to be present,
    /// and must not be combined with [`Content::encoding`].
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub item_encoding: Option<Encoding>,

    /// Optional extensions "x-something"
    #[serde(skip_serializing_if = "PropMap::is_empty", flatten)]
    pub extensions: PropMap<String, serde_json::Value>,
}

impl Content {
    /// Construct a new [`Content`].
    #[must_use]
    pub fn new<I: Into<RefOr<Schema>>>(schema: I) -> Self {
        Self {
            schema: Some(schema.into()),
            ..Self::default()
        }
    }

    /// Construct a [`Content`] that is purely a reference to a reusable Media Type Object.
    /// Requires OpenAPI 3.2.
    #[must_use]
    pub fn from_ref<S: Into<String>>(ref_location: S) -> Self {
        Self {
            ref_location: Some(ref_location.into()),
            ..Self::default()
        }
    }

    /// Set the `$ref` location for this [`Content`] and return `self`. Requires OpenAPI 3.2.
    #[must_use]
    pub fn ref_location<S: Into<String>>(mut self, ref_location: S) -> Self {
        self.ref_location = Some(ref_location.into());
        self
    }

    /// Add schema.
    #[must_use]
    pub fn schema<I: Into<RefOr<Schema>>>(mut self, component: I) -> Self {
        self.schema = Some(component.into());
        self
    }

    /// Add the schema describing each item of a sequential media type.
    /// See [`Content::item_schema`]. Requires OpenAPI 3.2.
    #[must_use]
    pub fn item_schema<I: Into<RefOr<Schema>>>(mut self, component: I) -> Self {
        self.item_schema = Some(component.into());
        self
    }

    /// Add example of schema.
    #[must_use]
    pub fn example(mut self, example: Value) -> Self {
        self.example = Some(example);
        self
    }

    /// Add iterator of _`(N, V)`_ where `N` is name of example and `V` is [`Example`][example] to
    /// [`Content`] of a request body or response body.
    ///
    /// [`Content::examples`] and [`Content::example`] are mutually exclusive. If both are defined
    /// `examples` will override value in `example`.
    ///
    /// [example]: ../example/Example.html
    #[must_use]
    pub fn extend_examples<
        E: IntoIterator<Item = (N, V)>,
        N: Into<String>,
        V: Into<RefOr<Example>>,
    >(
        mut self,
        examples: E,
    ) -> Self {
        self.examples.extend(
            examples
                .into_iter()
                .map(|(name, example)| (name.into(), example.into())),
        );

        self
    }

    /// Add openapi extensions (`x-something`) for [`Content`].
    #[must_use]
    pub fn extensions(mut self, extensions: PropMap<String, serde_json::Value>) -> Self {
        self.extensions = extensions;
        self
    }

    /// Add an encoding.
    ///
    /// The `property_name` MUST exist in the [`Content::schema`] as a property,
    /// with `schema` being a [`Schema::Object`] and this object containing the same property
    /// key in [`Object::properties`](crate::openapi::schema::Object::properties).
    ///
    /// The encoding object SHALL only apply to `request_body` objects when the media type is
    /// multipart or `application/x-www-form-urlencoded`.
    #[must_use]
    pub fn encoding<S: Into<String>, E: Into<Encoding>>(
        mut self,
        property_name: S,
        encoding: E,
    ) -> Self {
        self.encoding.insert(property_name.into(), encoding.into());
        self
    }

    /// Set the positional encodings. See [`Content::prefix_encoding`]. Requires OpenAPI 3.2.
    #[must_use]
    pub fn prefix_encoding<I: IntoIterator<Item = Encoding>>(mut self, prefix_encoding: I) -> Self {
        self.prefix_encoding = prefix_encoding.into_iter().collect();
        self
    }

    /// Set the encoding applied to remaining array items. See [`Content::item_encoding`].
    /// Requires OpenAPI 3.2.
    #[must_use]
    pub fn item_encoding<E: Into<Encoding>>(mut self, item_encoding: E) -> Self {
        self.item_encoding = Some(item_encoding.into());
        self
    }
}

impl From<RefOr<Schema>> for Content {
    fn from(schema: RefOr<Schema>) -> Self {
        Self {
            schema: Some(schema),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::{Map, json};

    use super::*;

    #[test]
    fn test_build_content() {
        let content = Content::new(RefOr::Ref(crate::Ref::from_schema_name("MySchema")))
            .example(Value::Object(Map::from_iter([(
                "schema".into(),
                Value::String("MySchema".to_owned()),
            )])))
            .encoding(
                "schema".to_owned(),
                Encoding::default().content_type("text/plain"),
            );
        assert_json_eq!(
            content,
            json!({
              "schema": {
                "$ref": "#/components/schemas/MySchema"
              },
              "example": {
                "schema": "MySchema"
              },
              "encoding": {
                  "schema": {
                    "contentType": "text/plain"
                  }
              }
            })
        );

        let content = content
            .schema(RefOr::Ref(crate::Ref::from_schema_name("NewSchema")))
            .extend_examples([(
                "example1".to_owned(),
                Example::new().value(Value::Object(Map::from_iter([(
                    "schema".into(),
                    Value::String("MySchema".to_owned()),
                )]))),
            )]);
        assert_json_eq!(
            content,
            json!({
              "schema": {
                "$ref": "#/components/schemas/NewSchema"
              },
              "example": {
                "schema": "MySchema"
              },
              "examples": {
                "example1": {
                  "value": {
                    "schema": "MySchema"
                  }
                }
              },
              "encoding": {
                  "schema": {
                    "contentType": "text/plain"
                  }
              }
            })
        );
    }

    #[test]
    fn content_ref_serializes_as_a_reference_object() {
        let content = Content::from_ref("#/components/mediaTypes/FramePayload");
        let value = serde_json::to_value(&content).expect("serialize");
        assert_json_eq!(
            &value,
            json!({ "$ref": "#/components/mediaTypes/FramePayload" })
        );

        let parsed: Content = serde_json::from_value(value).expect("deserialize");
        assert_eq!(parsed, content);
        assert!(parsed.schema.is_none());
    }

    #[test]
    fn test_content_openapi_3_2_streaming_fields() {
        let content = Content::default()
            .item_schema(crate::Ref::from_schema_name("Frame"))
            .prefix_encoding([Encoding::default().content_type("text/html")])
            .item_encoding(Encoding::default().content_type("image/*"));

        let value = serde_json::to_value(&content).expect("serialize");
        assert_json_eq!(
            &value,
            json!({
              "itemSchema": { "$ref": "#/components/schemas/Frame" },
              "prefixEncoding": [ { "contentType": "text/html" } ],
              "itemEncoding": { "contentType": "image/*" }
            })
        );

        let parsed: Content = serde_json::from_value(value).expect("deserialize");
        assert_eq!(parsed, content);
    }
}
