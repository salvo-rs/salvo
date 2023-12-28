//! Implements [OpenAPI Example Object][example] can be used to define examples for [`Response`][response]s and
//! [`RequestBody`][request_body]s.
//!
//! [example]: https://spec.openapis.org/oas/latest.html#example-object
//! [response]: response/struct.Response.html
//! [request_body]: request_body/struct.RequestBody.html
use serde::{Deserialize, Serialize};

/// Implements [OpenAPI Example Object][example].
///
/// Example is used on path operations to describe possible response bodies.
///
/// [example]: https://spec.openapis.org/oas/latest.html#example-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Example {
    /// Short description for the [`Example`].
    #[serde(skip_serializing_if = "String::is_empty")]
    pub summary: String,

    /// Long description for the [`Example`]. Value supports markdown syntax for rich text
    /// representation.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,

    /// Embedded literal example value. [`Example::value`] and [`Example::external_value`] are
    /// mutually exclusive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,

    /// An URI that points to a literal example value. [`Example::external_value`] provides the
    /// capability to references an example that cannot be easily included in JSON or YAML.
    /// [`Example::value`] and [`Example::external_value`] are mutually exclusive.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub external_value: String,
}

impl Example {
    /// Construct a new empty [`Example`]. This is effectively same as calling [`Example::default`].
    pub fn new() -> Self {
        Self::default()
    }
    /// Add or change a short description for the [`Example`]. Setting this to empty `String`
    /// will make it not render in the generated OpenAPI document.
    pub fn summary<S: Into<String>>(mut self, summary: S) -> Self {
        self.summary = summary.into();
        self
    }

    /// Add or change a long description for the [`Example`]. Markdown syntax is supported for rich
    /// text representation.
    ///
    /// Setting this to empty `String` will make it not render in the generated
    /// OpenAPI document.
    pub fn description<D: Into<String>>(mut self, description: D) -> Self {
        self.description = description.into();
        self
    }

    /// Add or change embedded literal example value. [`Example::value`] and [`Example::external_value`]
    /// are mutually exclusive.
    pub fn value(mut self, value: serde_json::Value) -> Self {
        self.value = Some(value);
        self
    }

    /// Add or change an URI that points to a literal example value. [`Example::external_value`]
    /// provides the capability to references an example that cannot be easily included
    /// in JSON or YAML. [`Example::value`] and [`Example::external_value`] are mutually exclusive.
    ///
    /// Setting this to an empty String will make the field not to render in the generated OpenAPI
    /// document.
    pub fn external_value<E: Into<String>>(mut self, external_value: E) -> Self {
        self.external_value = external_value.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        let example = Example::new();
        assert!(example.summary.is_empty());
        assert!(example.description.is_empty());
        assert!(example.value.is_none());
        assert!(example.external_value.is_empty());

        let example = example.summary("summary");
        assert!(example.summary == "summary");

        let example = example.description("description");
        assert!(example.description == "description");

        let example = example.external_value("external_value");
        assert!(example.external_value == "external_value");

        let example = example.value(serde_json::Value::String("value".to_string()));
        assert!(example.value.is_some());
        assert!(example.value.unwrap() == serde_json::Value::String("value".to_string()));
    }
}
