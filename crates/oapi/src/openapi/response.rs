//! Implements [OpenApi Responses][responses].
//!
//! [responses]: https://spec.openapis.org/oas/latest.html#responses-object
use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::IntoResponses;
use crate::{Ref, RefOr};

use super::{header::Header, set_value, Content};

/// Implements [OpenAPI Responses Object][responses].
///
/// Responses is a map holding api operation responses identified by their status code.
///
/// [responses]: https://spec.openapis.org/oas/latest.html#responses-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Responses {
    /// Map containing status code as a key with represented response as a value.
    #[serde(flatten)]
    pub responses: BTreeMap<String, RefOr<Response>>,
}

impl Responses {
    pub fn new() -> Self {
        Default::default()
    }
    /// Add a [`Response`].
    pub fn response<S: Into<String>, R: Into<RefOr<Response>>>(mut self, code: S, response: R) -> Self {
        self.responses.insert(code.into(), response.into());

        self
    }

    /// Add responses from an iterator over a pair of `(status_code, response): (String, Response)`.
    pub fn responses_from_iter<I: IntoIterator<Item = (C, R)>, C: Into<String>, R: Into<RefOr<Response>>>(
        mut self,
        iter: I,
    ) -> Self {
        self.responses
            .extend(iter.into_iter().map(|(code, response)| (code.into(), response.into())));
        self
    }

    /// Add responses from a type that implements [`IntoResponses`].
    pub fn responses_from_into_responses<I: IntoResponses>(mut self) -> Self {
        self.responses.extend(I::responses());
        self
    }
}

impl From<Responses> for BTreeMap<String, RefOr<Response>> {
    fn from(responses: Responses) -> Self {
        responses.responses
    }
}

impl<C, R> FromIterator<(C, R)> for Responses
where
    C: Into<String>,
    R: Into<RefOr<Response>>,
{
    fn from_iter<T: IntoIterator<Item = (C, R)>>(iter: T) -> Self {
        Self {
            responses: BTreeMap::from_iter(iter.into_iter().map(|(code, response)| (code.into(), response.into()))),
        }
    }
}

/// Implements [OpenAPI Response Object][response].
///
/// Response is api operation response.
///
/// [response]: https://spec.openapis.org/oas/latest.html#response-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    /// Description of the response. Response support markdown syntax.
    pub description: String,

    /// Map of headers identified by their name. `Content-Type` header will be ignored.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub headers: BTreeMap<String, Header>,

    /// Map of response [`Content`] objects identified by response body content type e.g `application/json`.
    ///
    /// [`Content`]s are stored within [`IndexMap`] to retain their insertion order. Swagger UI
    /// will create and show default example according to the first entry in `content` map.
    #[serde(skip_serializing_if = "IndexMap::is_empty", default)]
    pub content: IndexMap<String, Content>,
}

impl Response {
    /// Construct a new [`Response`].
    ///
    /// Function takes description as argument.
    pub fn new<S: Into<String>>(description: S) -> Self {
        Self {
            description: description.into(),
            ..Default::default()
        }
    }
    /// Add description. Description supports markdown syntax.
    pub fn description<I: Into<String>>(mut self, description: I) -> Self {
        set_value!(self description description.into())
    }

    /// Add [`Content`] of the [`Response`] with content type e.g `application/json`.
    pub fn content<S: Into<String>>(mut self, content_type: S, content: Content) -> Self {
        self.content.insert(content_type.into(), content);

        self
    }

    /// Add response [`Header`].
    pub fn header<S: Into<String>>(mut self, name: S, header: Header) -> Self {
        self.headers.insert(name.into(), header);

        self
    }
}

impl From<Ref> for RefOr<Response> {
    fn from(r: Ref) -> Self {
        Self::Ref(r)
    }
}

/// Trait with convenience functions for documenting response bodies.
///
/// With a single method call we can add [`Content`] to our [`Response`] and [`Response`]
/// that references a [schema][schema] using content-type `"application/json"`.
///
/// _**Add json response from schema ref.**_
/// ```
/// use salvo_oapi::response::{Response, ResponseExt};
///
/// let request = Response::new()
///     .description("A sample response")
///     .json_schema_ref("MyResponsePayload");
/// ```
///
/// If serialized to JSON, the above will result in a response schema like this.
/// ```json
/// {
///   "description": "A sample response",
///   "content": {
///     "application/json": {
///       "schema": {
///         "$ref": "#/components/schemas/MyResponsePayload"
///       }
///     }
///   }
/// }
/// ```
///
/// [response]: crate::ToResponse
/// [schema]: crate::ToSchema
///
#[cfg(feature = "openapi_extensions")]
#[cfg_attr(doc_cfg, doc(cfg(feature = "openapi_extensions")))]
pub trait ResponseExt {
    /// Add [`Content`] to [`Response`] referring to a _`schema`_
    /// with Content-Type `application/json`.
    fn json_schema_ref(self, ref_name: &str) -> Self;
}

#[cfg(feature = "openapi_extensions")]
impl ResponseExt for Response {
    fn json_schema_ref(mut self, ref_name: &str) -> Response {
        self.content.insert(
            "application/json".to_string(),
            Content::new(crate::Ref::from_schema_name(ref_name)),
        );
        self
    }
}

#[cfg(feature = "openapi_extensions")]
impl ResponseExt for Response {
    fn json_schema_ref(self, ref_name: &str) -> Response {
        self.content("application/json", Content::new(crate::Ref::from_schema_name(ref_name)))
    }
}

#[cfg(test)]
mod tests {
    use super::{Content, Response, Responses};
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    #[test]
    fn responses_new() {
        let responses = Responses::new();

        assert!(responses.responses.is_empty());
    }

    #[test]
    fn response_builder() -> Result<(), serde_json::Error> {
        let request_body = Response::new("A sample response").content(
            "application/json",
            Content::new(crate::Ref::from_schema_name("MySchemaPayload")),
        );
        let serialized = serde_json::to_string_pretty(&request_body)?;
        println!("serialized json:\n {serialized}");
        assert_json_eq!(
            request_body,
            json!({
              "description": "A sample response",
              "content": {
                "application/json": {
                  "schema": {
                    "$ref": "#/components/schemas/MySchemaPayload"
                  }
                }
              }
            })
        );
        Ok(())
    }
}

#[cfg(all(test, feature = "openapi_extensions"))]
mod openapi_extensions_tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use crate::Response;

    use super::ResponseExt;

    #[test]
    fn response_ext() {
        let request_body = Response::new("A sample response").json_schema_ref("MySchemaPayload");

        assert_json_eq!(
            request_body,
            json!({
              "description": "A sample response",
              "content": {
                "application/json": {
                  "schema": {
                    "$ref": "#/components/schemas/MySchemaPayload"
                  }
                }
              }
            })
        );
    }

    #[test]
    fn response_builder_ext() {
        let request_body = Response::new("A sample response").json_schema_ref("MySchemaPayload");
        assert_json_eq!(
            request_body,
            json!({
              "description": "A sample response",
              "content": {
                "application/json": {
                  "schema": {
                    "$ref": "#/components/schemas/MySchemaPayload"
                  }
                }
              }
            })
        );
    }
}
