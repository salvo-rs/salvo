//! Implements [OpenAPI Request Body][request_body] types.
//!
//! [request_body]: https://spec.openapis.org/oas/latest.html#request-body-object
use indexmap::IndexMap;
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
    #[serde(rename = "content")]
    pub contents: IndexMap<String, Content>,

    /// Determines whether request body is required in the request or not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Required>,
}

impl RequestBody {
    /// Construct a new empty [`RequestBody`]. This is effectively same as calling [`RequestBody::default`].
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
    pub fn add_content<S: Into<String>>(mut self, kind: S, content: Content) -> Self {
        self.contents.insert(kind.into(), content);
        self
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::{Content, RequestBody, Required};

    #[test]
    fn request_body_new() {
        let request_body = RequestBody::new();

        assert!(request_body.contents.is_empty());
        assert_eq!(request_body.description, None);
        assert!(request_body.required.is_none());
    }

    #[test]
    fn request_body_builder() -> Result<(), serde_json::Error> {
        let request_body = RequestBody::new()
            .description("A sample requestBody")
            .required(Required::True)
            .add_content(
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
