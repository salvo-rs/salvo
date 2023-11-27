//! Implements [OpenAPI Tag Object][tag] types.
//!
//! [tag]: https://spec.openapis.org/oas/latest.html#tag-object
use std::cmp::{Ord, Ordering, PartialOrd};
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::external_docs::ExternalDocs;

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

    /// Additional description for the tag shown in the document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Additional external documentation for the tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDocs>,

    /// Optional extensions "x-something"
    #[serde(skip_serializing_if = "Option::is_none", flatten)]
    pub extensions: Option<HashMap<String, serde_json::Value>>,
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
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }
    /// Add name fo the tag.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Add additional description for the tag.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add additional external documentation for the tag.
    pub fn external_docs(mut self, external_docs: ExternalDocs) -> Self {
        self.external_docs = Some(external_docs);
        self
    }
}
