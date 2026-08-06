//! Implements [OpenAPI Header Object][header] types.
//!
//! [header]: https://spec.openapis.org/oas/latest.html#header-object

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::parameter::ParameterStyle;
use super::{BasicType, Content, Deprecated, Example, Object, PropMap, RefOr, Schema};

/// Implements [OpenAPI Header Object][header] for response headers and for individual parts in
/// `multipart` representations.
///
/// A Header Object follows the structure of the [`Parameter`](crate::Parameter) object minus
/// `name` and `in`, and describes its value either through [`Header::schema`] or through
/// [`Header::content`].
///
/// [header]: https://spec.openapis.org/oas/latest.html#header-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    /// Additional description of the header value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Determines whether this header is mandatory. Defaults to `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,

    /// Declares the header deprecated and to be transitioned out of usage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Deprecated>,

    /// Schema of header type. Mutually exclusive with [`Header::content`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<RefOr<Schema>>,

    /// Describes how the header value is serialized. The only legal value for headers is
    /// [`ParameterStyle::Simple`], which is also the default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<ParameterStyle>,

    /// When `true`, `array` or `object` header values generate a single header whose value is a
    /// comma-separated list. Defaults to `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explode: Option<bool>,

    /// Example of the header's potential value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,

    /// Examples of the header's potential value, indexed by name. Mutually exclusive with
    /// [`Header::example`].
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub examples: PropMap<String, RefOr<Example>>,

    /// A map containing the representations for the header, keyed by media type. Per spec the
    /// map must contain exactly one entry. Mutually exclusive with [`Header::schema`].
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub content: PropMap<String, Content>,

    /// Optional extensions "x-something"
    #[serde(skip_serializing_if = "PropMap::is_empty", flatten)]
    pub extensions: PropMap<String, serde_json::Value>,
}

impl Header {
    /// Construct a new [`Header`] with custom schema. If you wish to construct a default
    /// header with `String` type you can use [`Header::default`] function.
    ///
    /// # Examples
    ///
    /// Creates a new [`Header`] with an integer type.
    /// ```
    /// # use salvo_oapi::{Header, Object, BasicType};
    /// let header = Header::new(Object::with_type(BasicType::Integer));
    /// ```
    ///
    /// Create a new [`Header`] with default type `String`
    /// ```
    /// # use salvo_oapi::Header;
    /// let header = Header::default();
    /// ```
    #[must_use]
    pub fn new<C: Into<RefOr<Schema>>>(component: C) -> Self {
        Self {
            schema: Some(component.into()),
            ..Default::default()
        }
    }

    /// Construct a [`Header`] that describes its value with a media type instead of a schema.
    ///
    /// ```
    /// # use salvo_oapi::{Content, Header, Object, BasicType};
    /// let header = Header::with_content(
    ///     "application/linkset",
    ///     Content::new(Object::with_type(BasicType::String)),
    /// );
    /// ```
    #[must_use]
    pub fn with_content<S: Into<String>, C: Into<Content>>(media_type: S, content: C) -> Self {
        let mut header = Self {
            schema: None,
            ..Default::default()
        };
        header.content.insert(media_type.into(), content.into());
        header
    }

    /// Add schema of header.
    #[must_use]
    pub fn schema<I: Into<RefOr<Schema>>>(mut self, component: I) -> Self {
        self.schema = Some(component.into());
        self
    }

    /// Add additional description for header.
    #[must_use]
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Declare whether the header is mandatory.
    #[must_use]
    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Declare the header deprecated.
    #[must_use]
    pub fn deprecated<D: Into<Deprecated>>(mut self, deprecated: D) -> Self {
        self.deprecated = Some(deprecated.into());
        self
    }

    /// Set the serialization style of the header. Only [`ParameterStyle::Simple`] is legal.
    #[must_use]
    pub fn style(mut self, style: ParameterStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Define whether `array` or `object` header values are exploded.
    #[must_use]
    pub fn explode(mut self, explode: bool) -> Self {
        self.explode = Some(explode);
        self
    }

    /// Add an example of the header's potential value.
    #[must_use]
    pub fn example(mut self, example: Value) -> Self {
        self.example = Some(example);
        self
    }

    /// Insert a named [`Example`] (or a [`Ref`](crate::Ref) to one) into [`Header::examples`].
    #[must_use]
    pub fn add_example<N: Into<String>, E: Into<RefOr<Example>>>(
        mut self,
        name: N,
        example: E,
    ) -> Self {
        self.examples.insert(name.into(), example.into());
        self
    }

    /// Insert a single media-type entry into [`Header::content`].
    ///
    /// Per spec the `content` map must contain exactly one entry. Mutually exclusive with
    /// [`Header::schema`].
    #[must_use]
    pub fn content<S: Into<String>, C: Into<Content>>(mut self, media_type: S, content: C) -> Self {
        self.content.insert(media_type.into(), content.into());
        self
    }

    /// Add openapi extension (`x-something`) for [`Header`].
    #[must_use]
    pub fn add_extension<K: Into<String>>(mut self, key: K, value: serde_json::Value) -> Self {
        self.extensions.insert(key.into(), value);
        self
    }
}

impl Default for Header {
    fn default() -> Self {
        Self {
            description: Default::default(),
            required: Default::default(),
            deprecated: Default::default(),
            schema: Some(Object::with_type(BasicType::String).into()),
            style: Default::default(),
            explode: Default::default(),
            example: Default::default(),
            examples: Default::default(),
            content: Default::default(),
            extensions: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_build_header() {
        let header = Header::new(Object::with_type(BasicType::String));
        assert_json_eq!(
            header,
            json!({
                "schema": {
                    "type": "string"
                }
            })
        );

        let header = header
            .description("test description")
            .schema(Object::with_type(BasicType::Number));
        assert_json_eq!(
            header,
            json!({
                "description": "test description",
                "schema": {
                    "type": "number"
                }
            })
        );
    }

    #[test]
    fn header_full_surface_round_trips() {
        let header = Header::new(Object::with_type(BasicType::String))
            .description("rate limit")
            .required(true)
            .deprecated(crate::Deprecated::False)
            .style(ParameterStyle::Simple)
            .explode(false)
            .example(json!("100"))
            .add_extension("x-vendor", json!("acme"));

        let value = serde_json::to_value(&header).expect("serialize");
        assert_json_eq!(
            &value,
            json!({
                "description": "rate limit",
                "required": true,
                "deprecated": false,
                "schema": { "type": "string" },
                "style": "simple",
                "explode": false,
                "example": "100",
                "x-vendor": "acme"
            })
        );

        let parsed: Header = serde_json::from_value(value).expect("deserialize");
        assert_eq!(parsed, header);
    }

    #[test]
    fn header_with_content_omits_schema() {
        let header = Header::with_content(
            "application/linkset",
            Content::new(Object::with_type(BasicType::String)),
        );

        let value = serde_json::to_value(&header).expect("serialize");
        assert_json_eq!(
            &value,
            json!({
                "content": {
                    "application/linkset": { "schema": { "type": "string" } }
                }
            })
        );

        let parsed: Header = serde_json::from_value(value).expect("deserialize");
        assert_eq!(parsed, header);
    }
}
