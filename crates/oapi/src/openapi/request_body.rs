//! Implements [OpenAPI Request Body][request_body] types.
//!
//! [request_body]: https://spec.openapis.org/oas/latest.html#request-body-object
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{set_value, Content, Required};

/// Implements [OpenAPI Request Body][request_body].
///
/// [request_body]: https://spec.openapis.org/oas/latest.html#request-body-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestBody {
    /// Additional description of [`RequestBody`] supporting markdown syntax.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Map of request body contents mapped by content type e.g. `application/json`.
    pub content: BTreeMap<String, Content>,

    /// Determines whether request body is required in the request or not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Required>,
}

impl RequestBody {
    /// Construct a new [`RequestBody`].
    pub fn new() -> Self {
        Default::default()
    }
    /// Add description for [`RequestBody`].
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        set_value!(self description Some(description.into()))
    }

    /// Define [`RequestBody`] required.
    pub fn required(mut self, required: Required) -> Self {
        set_value!(self required Some(required))
    }

    /// Add [`Content`] by content type e.g `application/json` to [`RequestBody`].
    pub fn content<S: Into<String>>(mut self, content_type: S, content: Content) -> Self {
        self.content.insert(content_type.into(), content);

        self
    }
}

/// Trait with convenience functions for documenting request bodies.
///
/// With a single method call we can add [`Content`] to our [`RequestBody`] and
/// [`RequestBody`] that references a [schema][schema] using
/// content-type `"application/json"`.
///
/// _**Add json request body from schema ref.**_
/// ```
/// use salvo_oapi::request_body::{RequestBody, RequestBodyExt};
///
/// let request = RequestBody::new().json_schema_ref("EmailPayload");
/// ```
///
/// If serialized to JSON, the above will result in a requestBody schema like this.
/// ```json
/// {
///   "content": {
///     "application/json": {
///       "schema": {
///         "$ref": "#/components/schemas/EmailPayload"
///       }
///     }
///   }
/// }
/// ```
///
/// [schema]: crate::ToSchema
///
#[cfg(feature = "openapi_extensions")]
#[cfg_attr(doc_cfg, doc(cfg(feature = "openapi_extensions")))]
pub trait RequestBodyExt {
    /// Add [`Content`] to [`RequestBody`] referring to a _`schema`_
    /// with Content-Type `application/json`.
    fn json_schema_ref(self, ref_name: &str) -> Self;
}

#[cfg(feature = "openapi_extensions")]
impl RequestBodyExt for RequestBody {
    fn json_schema_ref(mut self, ref_name: &str) -> RequestBody {
        self.content.insert(
            "application/json".to_string(),
            crate::Content::new(crate::Ref::from_schema_name(ref_name)),
        );
        self
    }
}

#[cfg(feature = "openapi_extensions")]
impl RequestBodyExt for RequestBody {
    fn json_schema_ref(self, ref_name: &str) -> RequestBody {
        self.content(
            "application/json",
            crate::Content::new(crate::Ref::from_schema_name(ref_name)),
        )
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::{Content, RequestBody, RequestBody, Required};

    #[test]
    fn request_body_new() {
        let request_body = RequestBody::new();

        assert!(request_body.content.is_empty());
        assert_eq!(request_body.description, None);
        assert!(request_body.required.is_none());
    }

    #[test]
    fn request_body_builder() -> Result<(), serde_json::Error> {
        let request_body = RequestBody::new()
            .description("A sample requestBody")
            .required(Required::True)
            .content(
                "application/json",
                Content::new(crate::Ref::from_schema_name("EmailPayload")),
            );
        let serialized = serde_json::to_string_pretty(&request_body)?;
        println!("serialized json:\n {serialized}");
        assert_json_eq!(
            request_body,
            json!({
              "description": "A sample requestBody",
              "content": {
                "application/json": {
                  "schema": {
                    "$ref": "#/components/schemas/EmailPayload"
                  }
                }
              },
              "required": true
            })
        );
        Ok(())
    }
}

#[cfg(all(test, feature = "openapi_extensions"))]
#[cfg_attr(doc_cfg, doc(cfg(feature = "openapi_extensions")))]
mod openapi_extensions_tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use crate::request_body::RequestBody;

    use super::RequestBodyExt;

    #[test]
    fn request_body_ext() {
        let request_body = RequestBody::new()
            // build a RequestBody first to test the method
            .json_schema_ref("EmailPayload");
        assert_json_eq!(
            request_body,
            json!({
              "content": {
                "application/json": {
                  "schema": {
                    "$ref": "#/components/schemas/EmailPayload"
                  }
                }
              }
            })
        );
    }

    #[test]
    fn request_body_builder_ext() {
        let request_body = RequestBody::new().json_schema_ref("EmailPayload");
        assert_json_eq!(
            request_body,
            json!({
              "content": {
                "application/json": {
                  "schema": {
                    "$ref": "#/components/schemas/EmailPayload"
                  }
                }
              }
            })
        );
    }
}
