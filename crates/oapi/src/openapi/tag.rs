//! Implements [OpenAPI Tag Object][tag] types.
//!
//! [tag]: https://spec.openapis.org/oas/latest.html#tag-object
use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::external_docs::ExternalDocs;
use crate::PropMap;

/// Implements [OpenAPI Tag Object][tag].
///
/// Tag can be used to provide additional metadata for tags used by path operations.
///
/// [tag]: https://spec.openapis.org/oas/latest.html#tag-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    /// Name of the tag. Should match to tag of **operation**.
    pub name: String,

    /// Short summary of the tag, used for display purposes. Added in OpenAPI 3.2 as the
    /// standardized replacement for the `x-displayName` extension.
    ///
    /// See <https://spec.openapis.org/oas/v3.2.0.html#tag-object>.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// Additional description for the tag shown in the document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Additional external documentation for the tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDocs>,

    /// [`Tag::name`] of the tag this tag is nested under. Added in OpenAPI 3.2.
    ///
    /// The named tag must exist in the document and circular parent/child references are not
    /// allowed; neither condition is validated here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,

    /// Machine-readable string categorizing what sort of tag this is. Added in OpenAPI 3.2.
    ///
    /// Any string is allowed; commonly used values are `nav`, `badge` and `audience`. See the
    /// [registry](https://spec.openapis.org/registry/tag-kind/) for the well-known values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,

    /// Optional extensions "x-something"
    #[serde(skip_serializing_if = "PropMap::is_empty", flatten)]
    pub extensions: PropMap<String, serde_json::Value>,
}
impl Ord for Tag {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}
impl PartialOrd for Tag {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl From<String> for Tag {
    fn from(name: String) -> Self {
        Self::new(name)
    }
}
impl From<&String> for Tag {
    fn from(name: &String) -> Self {
        Self::new(name)
    }
}
impl<'a> From<&'a str> for Tag {
    fn from(name: &'a str) -> Self {
        Self::new(name.to_owned())
    }
}

impl Tag {
    /// Construct a new [`Tag`] with given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }
    /// Add name of the tag.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Add a short summary used for display purposes. Requires OpenAPI 3.2.
    #[must_use]
    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Add additional description for the tag.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Nest this tag under the tag with the given name. Requires OpenAPI 3.2.
    #[must_use]
    pub fn parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    /// Categorize the tag, e.g. `nav`, `badge` or `audience`. Requires OpenAPI 3.2.
    #[must_use]
    pub fn kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = Some(kind.into());
        self
    }

    /// Add additional external documentation for the tag.
    #[must_use]
    pub fn external_docs(mut self, external_docs: ExternalDocs) -> Self {
        self.external_docs = Some(external_docs);
        self
    }

    /// Add openapi extension (`x-something`) for [`Tag`].
    #[must_use]
    pub fn add_extension<K: Into<String>>(mut self, key: K, value: serde_json::Value) -> Self {
        self.extensions.insert(key.into(), value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{ExternalDocs, Tag};

    #[test]
    fn tag_new() {
        let tag = Tag::new("tag name");
        assert_eq!(tag.name, "tag name");
        assert!(tag.description.is_none());
        assert!(tag.external_docs.is_none());
        assert!(tag.extensions.is_empty());

        let tag = tag.name("new tag name");
        assert_eq!(tag.name, "new tag name");

        let tag = tag.description("description");
        assert!(tag.description.is_some());

        let tag = tag.external_docs(ExternalDocs::new(""));
        assert!(tag.external_docs.is_some());
    }

    #[test]
    fn tag_openapi_3_2_fields_round_trip() {
        let tag = Tag::new("partner")
            .summary("Partner")
            .description("Operations available to the partners network")
            .parent("external")
            .kind("audience");

        let value = serde_json::to_value(&tag).expect("serialize");
        assert_eq!(
            value,
            serde_json::json!({
                "name": "partner",
                "summary": "Partner",
                "description": "Operations available to the partners network",
                "parent": "external",
                "kind": "audience"
            })
        );

        let parsed: Tag = serde_json::from_value(value).expect("deserialize");
        assert_eq!(parsed, tag);
    }

    #[test]
    fn tag_3_1_output_is_unchanged() {
        let tag = Tag::new("pets").description("pet operations");
        assert_eq!(
            serde_json::to_value(&tag).expect("serialize"),
            serde_json::json!({ "name": "pets", "description": "pet operations" })
        );
    }

    #[test]
    fn from_string() {
        let name = "tag name".to_owned();
        let tag = Tag::from(name);
        assert_eq!(tag.name, "tag name".to_owned());
    }

    #[test]
    fn from_string_ref() {
        let name = "tag name".to_owned();
        let tag = Tag::from(&name);
        assert_eq!(tag.name, "tag name".to_owned());
    }

    #[test]
    fn from_str() {
        let name = "tag name";
        let tag = Tag::from(name);
        assert_eq!(tag.name, "tag name");
    }

    #[test]
    fn cmp() {
        let tag1 = Tag::new("a");
        let tag2 = Tag::new("b");

        assert!(tag1 < tag2);
    }
}
