//! Implements [OpenApi Responses][responses].
//!
//! [responses]: https://spec.openapis.org/oas/latest.html#responses-object
use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{Ref, RefOr};

use super::{header::Header, set_value, Content};

/// Implements [OpenAPI Responses Object][responses].
///
/// Responses is a map holding api operation responses identified by their status code.
///
/// [responses]: https://spec.openapis.org/oas/latest.html#responses-object
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Responses(BTreeMap<String, RefOr<Response>>);

impl<K, R> From<BTreeMap<K, R>> for Responses
where
    K: Into<String>,
    R: Into<RefOr<Response>>,
{
    fn from(inner: BTreeMap<K, R>) -> Self {
        Self(inner.into_iter().map(|(k, v)| (k.into(), v.into())).collect())
    }
}
impl<K, R, const N: usize> From<[(K, R); N]> for Responses
where
    K: Into<String>,
    R: Into<RefOr<Response>>,
{
    fn from(inner: [(K, R); N]) -> Self {
        Self(
            <[(K, R)]>::into_vec(Box::new(inner))
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}

impl Deref for Responses {
    type Target = BTreeMap<String, RefOr<Response>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Responses {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for Responses {
    type Item = (String, RefOr<Response>);
    type IntoIter = <BTreeMap<String, RefOr<Response>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Responses {
    /// Construct a new empty [`Responses`]. This is effectively same as calling [`Responses::default`].
    pub fn new() -> Self {
        Default::default()
    }
    /// Inserts a key-value pair into the instance and retuns `self`.
    pub fn response<S: Into<String>, R: Into<RefOr<Response>>>(mut self, code: S, response: R) -> Self {
        self.insert(code, response);
        self
    }

    /// Inserts a key-value pair into the instance.
    pub fn insert<S: Into<String>, R: Into<RefOr<Response>>>(&mut self, code: S, response: R) {
        self.0.insert(code.into(), response.into());
    }

    /// Moves all elements from `other` into `self`, leaving `other` empty.
    ///
    /// If a key from `other` is already present in `self`, the respective
    /// value from `self` will be overwritten with the respective value from `other`.
    pub fn append(&mut self, other: &mut Responses) {
        self.0.append(&mut other.0);
    }

    /// Add responses from an iterator over a pair of `(status_code, response): (String, Response)`.
    pub fn extend<I, C, R>(&mut self, iter: I)
    where
        I: IntoIterator<Item = (C, R)>,
        C: Into<String>,
        R: Into<RefOr<Response>>,
    {
        self.0
            .extend(iter.into_iter().map(|(code, response)| (code.into(), response.into())));
    }
}

impl From<Responses> for BTreeMap<String, RefOr<Response>> {
    fn from(responses: Responses) -> Self {
        responses.0
    }
}

impl<C, R> FromIterator<(C, R)> for Responses
where
    C: Into<String>,
    R: Into<RefOr<Response>>,
{
    fn from_iter<T: IntoIterator<Item = (C, R)>>(iter: T) -> Self {
        Self(BTreeMap::from_iter(
            iter.into_iter().map(|(code, response)| (code.into(), response.into())),
        ))
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
    #[serde(rename = "content")]
    pub contents: IndexMap<String, Content>,
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
    pub fn add_content<S: Into<String>>(mut self, kind: S, content: Content) -> Self {
        self.contents.insert(kind.into(), content);

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

#[cfg(test)]
mod tests {
    use super::{Content, Response, Responses};
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    #[test]
    fn responses_new() {
        let responses = Responses::new();
        assert!(responses.is_empty());
    }

    #[test]
    fn response_builder() -> Result<(), serde_json::Error> {
        let request_body = Response::new("A sample response").add_content(
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
