//! Implements content object for request body and response.
use serde::{Deserialize, Serialize};

use serde_json::Value;

use super::PropMap;
use super::example::Example;
use super::{RefOr, Schema, encoding::Encoding};

/// Content holds request body content or response content.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Content {
    /// Schema used in response body or request body.
    pub schema: RefOr<Schema>,

    /// Example for request body or response body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,

    /// Examples of the request body or response body. [`Content::examples`] should match to
    /// media type and specified schema if present. [`Content::examples`] and
    /// [`Content::example`] are mutually exclusive. If both are defined `examples` will
    /// override value in `example`.
    #[serde(skip_serializing_if = "PropMap::is_empty")]
    pub examples: PropMap<String, RefOr<Example>>,

    /// A map between a property name and its encoding information.
    ///
    /// The key, being the property name, MUST exist in the [`Content::schema`] as a property, with
    /// `schema` being a [`Schema::Object`] and this object containing the same property key in
    /// [`Object::properties`](crate::schema::Object::properties).
    ///
    /// The encoding object SHALL only apply to `request_body` objects when the media type is
    /// multipart or `application/x-www-form-urlencoded`.
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub encoding: PropMap<String, Encoding>,
}

impl Content {
    /// Construct a new [`Content`].
    pub fn new<I: Into<RefOr<Schema>>>(schema: I) -> Self {
        Self {
            schema: schema.into(),
            ..Self::default()
        }
    }

    /// Add schema.
    pub fn schema<I: Into<RefOr<Schema>>>(mut self, component: I) -> Self {
        self.schema = component.into();
        self
    }

    /// Add example of schema.
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

    /// Add an encoding.
    ///
    /// The `property_name` MUST exist in the [`Content::schema`] as a property,
    /// with `schema` being a [`Schema::Object`] and this object containing the same property
    /// key in [`Object::properties`](crate::openapi::schema::Object::properties).
    ///
    /// The encoding object SHALL only apply to `request_body` objects when the media type is
    /// multipart or `application/x-www-form-urlencoded`.
    pub fn encoding<S: Into<String>, E: Into<Encoding>>(
        mut self,
        property_name: S,
        encoding: E,
    ) -> Self {
        self.encoding.insert(property_name.into(), encoding.into());
        self
    }
}

impl From<RefOr<Schema>> for Content {
    fn from(schema: RefOr<Schema>) -> Self {
        Self {
            schema,
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
                Value::String("MySchema".to_string()),
            )])))
            .encoding(
                "schema".to_string(),
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
                "example1".to_string(),
                Example::new().value(Value::Object(Map::from_iter([(
                    "schema".into(),
                    Value::String("MySchema".to_string()),
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
}
