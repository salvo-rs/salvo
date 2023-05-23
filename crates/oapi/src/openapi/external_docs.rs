//! Implements [OpenAPI External Docs Object][external_docs] types.
//!
//! [external_docs]: https://spec.openapis.org/oas/latest.html#xml-object
use serde::{Deserialize, Serialize};

/// Reference of external resource allowing extended documentation.
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExternalDocs {
    /// Target url for external documentation location.
    pub url: String,
    /// Additional description supporting markdown syntax of the external documentation.
    pub description: Option<String>,
}

impl ExternalDocs {
    /// Construct a new [`ExternalDocs`].
    ///
    /// Function takes target url argument for the external documentation location.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_oapi::ExternalDocs;
    /// let external_docs = ExternalDocs::new("https://pet-api.external.docs");
    /// ```
    pub fn new<S: AsRef<str>>(url: S) -> Self {
        Self {
            url: url.as_ref().to_string(),
            ..Default::default()
        }
    }

    /// Add target url for external documentation location.
    pub fn url<I: Into<String>>(mut self, url: I) -> Self {
        self.url = url.into();
        self
    }

    /// Add additional description of external documentation.
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }
}
