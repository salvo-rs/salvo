//! Implements encoding object for content.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::parameter::ParameterStyle;
use super::Header;

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
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, Header>,

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

    /// Determines whether the parameter value SHOULD allow reserved characters, as defined by
    /// RFC3986 `:/?#[]@!$&'()*+,;=` to be included without percent-encoding. The default value is
    /// false. This property SHALL be ignored if the request body media type is not
    /// `application/x-www-form-urlencoded`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_reserved: Option<bool>,
}

impl Encoding {
    /// Set the content type. See [`Encoding::content_type`].
    pub fn content_type<S: Into<String>>(mut self, content_type: S) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Add a [`Header`]. See [`Encoding::headers`].
    pub fn header<S: Into<String>, H: Into<Header>>(mut self, header_name: S, header: H) -> Self {
        self.headers.insert(header_name.into(), header.into());

        self
    }

    /// Set the style [`ParameterStyle`]. See [`Encoding::style`].
    pub fn style(mut self, style: ParameterStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Set the explode. See [`Encoding::explode`].
    pub fn explode(mut self, explode: bool) -> Self {
        self.explode = Some(explode);
        self
    }

    /// Set the allow reserved. See [`Encoding::allow_reserved`].
    pub fn allow_reserved(mut self, allow_reserved: bool) -> Self {
        self.allow_reserved = Some(allow_reserved);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

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
}
