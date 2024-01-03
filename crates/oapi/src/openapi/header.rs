//! Implements [OpenAPI Header Object][header] types.
//!
//! [header]: https://spec.openapis.org/oas/latest.html#header-object

use serde::{Deserialize, Serialize};

use super::{Object, RefOr, Schema, SchemaType};

/// Implements [OpenAPI Header Object][header] for response headers.
///
/// [header]: https://spec.openapis.org/oas/latest.html#header-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Header {
    /// Schema of header type.
    pub schema: RefOr<Schema>,

    /// Additional description of the header value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Header {
    /// Construct a new [`Header`] with custom schema. If you wish to construct a default
    /// header with `String` type you can use [`Header::default`] function.
    ///
    /// # Examples
    ///
    /// Create new [`Header`] with integer type.
    /// ```
    /// # use salvo_oapi::{Header, Object, SchemaType};
    /// let header = Header::new(Object::with_type(SchemaType::Integer));
    /// ```
    ///
    /// Create a new [`Header`] with default type `String`
    /// ```
    /// # use salvo_oapi::Header;
    /// let header = Header::default();
    /// ```
    pub fn new<C: Into<RefOr<Schema>>>(component: C) -> Self {
        Self {
            schema: component.into(),
            ..Default::default()
        }
    }
    /// Add schema of header.
    pub fn schema<I: Into<RefOr<Schema>>>(mut self, component: I) -> Self {
        self.schema = component.into();
        self
    }

    /// Add additional description for header.
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl Default for Header {
    fn default() -> Self {
        Self {
            description: Default::default(),
            schema: Object::with_type(SchemaType::String).into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_build_header() {
        let header = Header::new(Object::with_type(SchemaType::String));
        assert_json_eq!(
            header,
            json!({
                "schema": {
                    "type": "string"
                }
            })
        );

        let header = header
            .description("test description")
            .schema(Object::with_type(SchemaType::Number));
        assert_json_eq!(
            header,
            json!({
                "description": "test description",
                "schema": {
                    "type": "number"
                }
            })
        );
    }
}
