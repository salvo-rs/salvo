//! Implements encoding object for content.

use serde::{Deserialize, Serialize};

use super::parameter::ParameterStyle;
use super::{Header, PropMap};

/// A single encoding definition applied to a single schema [`Object
/// property`](crate::openapi::schema::Object::properties).
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Encoding {
    /// The Content-Type for encoding a specific property. Default value depends on the property
    /// type: for string with format being binary – `application/octet-stream`; for other primitive
    /// types – `text/plain`; for object - `application/json`; for array – the default is defined
    /// based on the inner type. The value can be a specific media type (e.g. `application/json`),
    /// a wildcard media type (e.g. `image/*`), or a comma-separated list of the two types.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,

    /// A map allowing additional information to be provided as headers, for example
    /// Content-Disposition. Content-Type is described separately and SHALL be ignored in this
    /// section. This property SHALL be ignored if the request body media type is not a multipart.
    #[serde(default, skip_serializing_if = "PropMap::is_empty")]
    pub headers: PropMap<String, Header>,

    /// Describes how a specific property value will be serialized depending on its type. See
    /// Parameter Object for details on the style property. The behavior follows the same values as
    /// query parameters, including default values. This property SHALL be ignored if the request
    /// body media type is not `application/x-www-form-urlencoded`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<ParameterStyle>,

    /// When this is true, property values of type array or object generate separate parameters for
    /// each value of the array, or key-value-pair of the map. For other types of properties this
    /// property has no effect. When style is form, the default value is true. For all other
    /// styles, the default value is false. This property SHALL be ignored if the request body
    /// media type is not `application/x-www-form-urlencoded`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explode: Option<bool>,

    /// When this is true, values are serialized using reserved expansion, letting RFC3986's
    /// reserved character set `:/?#[]@!$&'()*+,;=` pass through unchanged. The default value is
    /// false.
    ///
    /// In OpenAPI 3.1 this only applied to `application/x-www-form-urlencoded` request bodies;
    /// OpenAPI 3.2 generalizes it to RFC6570-style serialization, and it has no effect for
    /// `multipart/form-data`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_reserved: Option<bool>,

    /// Nested encoding applied by property name, mirroring the [`Content::encoding`] field of the
    /// enclosing media type. Added in OpenAPI 3.2.
    ///
    /// Must not be combined with [`Encoding::prefix_encoding`] or [`Encoding::item_encoding`].
    ///
    /// [`Content::encoding`]: crate::openapi::Content::encoding
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub encoding: PropMap<String, Encoding>,

    /// Nested positional encoding, mirroring the [`Content::prefix_encoding`] field of the
    /// enclosing media type. Added in OpenAPI 3.2.
    ///
    /// [`Content::prefix_encoding`]: crate::openapi::Content::prefix_encoding
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub prefix_encoding: Vec<Encoding>,

    /// Nested encoding applied to every remaining array item, mirroring the
    /// [`Content::item_encoding`] field of the enclosing media type. Added in OpenAPI 3.2.
    ///
    /// [`Content::item_encoding`]: crate::openapi::Content::item_encoding
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub item_encoding: Option<Box<Encoding>>,

    /// Optional extensions "x-something"
    #[serde(skip_serializing_if = "PropMap::is_empty", flatten)]
    pub extensions: PropMap<String, serde_json::Value>,
}

impl Encoding {
    /// Set the content type. See [`Encoding::content_type`].
    #[must_use]
    pub fn content_type<S: Into<String>>(mut self, content_type: S) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Add a [`Header`]. See [`Encoding::headers`].
    #[must_use]
    pub fn header<S: Into<String>, H: Into<Header>>(mut self, header_name: S, header: H) -> Self {
        self.headers.insert(header_name.into(), header.into());

        self
    }

    /// Set the style [`ParameterStyle`]. See [`Encoding::style`].
    #[must_use]
    pub fn style(mut self, style: ParameterStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Set the explode. See [`Encoding::explode`].
    #[must_use]
    pub fn explode(mut self, explode: bool) -> Self {
        self.explode = Some(explode);
        self
    }

    /// Set the allow reserved. See [`Encoding::allow_reserved`].
    #[must_use]
    pub fn allow_reserved(mut self, allow_reserved: bool) -> Self {
        self.allow_reserved = Some(allow_reserved);
        self
    }

    /// Add a nested [`Encoding`] by property name. See [`Encoding::encoding`].
    /// Requires OpenAPI 3.2.
    #[must_use]
    pub fn encoding<S: Into<String>, E: Into<Self>>(
        mut self,
        property_name: S,
        encoding: E,
    ) -> Self {
        self.encoding.insert(property_name.into(), encoding.into());
        self
    }

    /// Set the nested positional encodings. See [`Encoding::prefix_encoding`].
    /// Requires OpenAPI 3.2.
    #[must_use]
    pub fn prefix_encoding<I: IntoIterator<Item = Self>>(mut self, prefix_encoding: I) -> Self {
        self.prefix_encoding = prefix_encoding.into_iter().collect();
        self
    }

    /// Set the nested encoding applied to remaining array items. See [`Encoding::item_encoding`].
    /// Requires OpenAPI 3.2.
    #[must_use]
    pub fn item_encoding<E: Into<Self>>(mut self, item_encoding: E) -> Self {
        self.item_encoding = Some(Box::new(item_encoding.into()));
        self
    }

    /// Add openapi extensions (`x-something`) for [`Encoding`].
    #[must_use]
    pub fn extensions(mut self, extensions: PropMap<String, serde_json::Value>) -> Self {
        self.extensions = extensions;
        self
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_encoding_default() {
        let encoding = Encoding::default();
        assert_json_eq!(encoding, json!({}));
    }

    #[test]
    fn test_build_encoding() {
        let encoding = Encoding::default()
            .content_type("application/json")
            .header("header1", Header::default())
            .style(ParameterStyle::Simple)
            .explode(true)
            .allow_reserved(false);

        assert_json_eq!(
            encoding,
            json!({
              "contentType": "application/json",
              "headers": {
                "header1": {
                  "schema": {
                    "type": "string"
                  }
                }
              },
              "style": "simple",
              "explode": true,
              "allowReserved": false
            })
        );
    }

    #[test]
    fn test_nested_encoding_openapi_3_2() {
        let encoding = Encoding::default()
            .content_type("multipart/mixed")
            .prefix_encoding([Encoding::default().content_type("text/html")])
            .item_encoding(Encoding::default().content_type("image/*"))
            .encoding("thumbnail", Encoding::default().content_type("image/png"));

        let value = serde_json::to_value(&encoding).expect("serialize");
        assert_json_eq!(
            &value,
            json!({
              "contentType": "multipart/mixed",
              "encoding": { "thumbnail": { "contentType": "image/png" } },
              "prefixEncoding": [ { "contentType": "text/html" } ],
              "itemEncoding": { "contentType": "image/*" }
            })
        );

        let parsed: Encoding = serde_json::from_value(value).expect("deserialize");
        assert_eq!(parsed, encoding);
    }
}
