//! Implements [OpenAPI Tag Object][tag] types.
//!
//! [tag]: https://spec.openapis.org/oas/latest.html#tag-object
use std::cmp::{Ord, Ordering, PartialOrd};

use serde::{Deserialize, Serialize};

use super::{external_docs::ExternalDocs, set_value};

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

impl Tag {
    /// Construct a new [`Tag`] with given name.
    pub fn new(name: impl AsRef<str>) -> Self {
        Self {
            name: name.as_ref().to_string(),
            ..Default::default()
        }
    }
    /// Add name fo the tag.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        set_value!(self name name.into())
    }

    /// Add additional description for the tag.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        set_value!(self description Some(description.into()))
    }

    /// Add additional external documentation for the tag.
    pub fn external_docs(mut self, external_docs: ExternalDocs) -> Self {
        set_value!(self external_docs Some(external_docs))
    }
}
