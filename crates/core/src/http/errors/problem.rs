use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};

use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::{Map, Value};

use crate::http::header::{CONTENT_TYPE, HeaderValue};
use crate::http::{Response, StatusCode, StatusError};
use crate::Scribe;

/// The media type for an RFC 9457 JSON problem details document.
pub const PROBLEM_JSON: &str = "application/problem+json";

const ABOUT_BLANK: &str = "about:blank";
const STANDARD_MEMBERS: [&str; 5] = ["type", "title", "status", "detail", "instance"];

/// An [RFC 9457] problem details response.
///
/// The Rust field and builder for the RFC's `type` member are named [`kind`](Self::kind) because
/// `type` is a Rust keyword. It is still serialized as `type` on the wire.
///
/// `Problem::new` creates an `about:blank` problem. Use [`Problem::kind`] to identify a more
/// specific problem type and [`Problem::extension`] for problem-specific extension members.
///
/// # Example
///
/// ```
/// use salvo_core::http::{Problem, StatusCode};
/// use serde_json::json;
///
/// let problem = Problem::new(StatusCode::UNPROCESSABLE_ENTITY)
///     .kind("https://example.com/problems/validation-error")
///     .title("The request is not valid")
///     .detail("The age field must be a positive integer")
///     .instance("/problems/123")
///     .extension("errors", json!([{"pointer": "#/age"}]));
///
/// assert_eq!(problem.status, StatusCode::UNPROCESSABLE_ENTITY);
/// assert_eq!(problem.kind, "https://example.com/problems/validation-error");
/// ```
///
/// [RFC 9457]: https://www.rfc-editor.org/rfc/rfc9457.html
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Problem {
    /// A URI reference that identifies the problem type.
    ///
    /// This is serialized as the RFC 9457 `type` member.
    pub kind: String,
    /// A short, human-readable summary of the problem type.
    pub title: String,
    /// The HTTP status code for this occurrence of the problem.
    ///
    /// Rendering the problem also uses this value as the HTTP response status.
    pub status: StatusCode,
    /// A human-readable explanation specific to this occurrence of the problem.
    pub detail: Option<String>,
    /// A URI reference that identifies this specific occurrence of the problem.
    pub instance: Option<String>,
    extensions: Map<String, Value>,
}

impl Problem {
    /// Creates an `about:blank` problem for `status`.
    ///
    /// Its title is initialized from the status code's canonical reason phrase.
    #[must_use]
    pub fn new(status: StatusCode) -> Self {
        Self {
            kind: ABOUT_BLANK.into(),
            title: status.canonical_reason().unwrap_or("Unknown Status").into(),
            status,
            detail: None,
            instance: None,
            extensions: Map::new(),
        }
    }

    /// Sets the URI reference that identifies the problem type.
    #[must_use]
    pub fn kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = kind.into();
        self
    }

    /// Sets the short, human-readable summary of the problem type.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets the explanation specific to this occurrence of the problem.
    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Sets the URI reference that identifies this occurrence of the problem.
    #[must_use]
    pub fn instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    /// Adds a problem-specific extension member.
    ///
    /// RFC 9457 extension members are serialized at the top level. Standard member names are
    /// reserved and are ignored here so an extension cannot replace their values.
    #[must_use]
    pub fn extension(mut self, name: impl Into<String>, value: Value) -> Self {
        let name = name.into();
        if !STANDARD_MEMBERS.contains(&name.as_str()) {
            self.extensions.insert(name, value);
        }
        self
    }

    /// Returns the problem-specific extension members.
    #[must_use]
    pub fn extensions(&self) -> &Map<String, Value> {
        &self.extensions
    }
}

impl Serialize for Problem {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut length = 3 + self.extensions.len();
        length += usize::from(self.detail.is_some()) + usize::from(self.instance.is_some());
        let mut map = serializer.serialize_map(Some(length))?;
        map.serialize_entry("type", &self.kind)?;
        map.serialize_entry("title", &self.title)?;
        map.serialize_entry("status", &self.status.as_u16())?;
        if let Some(detail) = &self.detail {
            map.serialize_entry("detail", detail)?;
        }
        if let Some(instance) = &self.instance {
            map.serialize_entry("instance", instance)?;
        }
        for (name, value) in &self.extensions {
            map.serialize_entry(name, value)?;
        }
        map.end()
    }
}

impl Scribe for Problem {
    fn render(self, res: &mut Response) {
        let status = self.status;
        match serde_json::to_vec(&self) {
            Ok(data) => {
                res.status_code(status);
                res.headers_mut().insert(
                    CONTENT_TYPE,
                    HeaderValue::from_static(PROBLEM_JSON),
                );
                res.body(data);
            }
            Err(error) => {
                tracing::error!(error = ?error, "problem details serialization failed");
                res.render(StatusError::internal_server_error().cause(error));
            }
        }
    }
}

impl From<StatusCode> for Problem {
    fn from(status: StatusCode) -> Self {
        Self::new(status)
    }
}

impl From<&StatusError> for Problem {
    fn from(error: &StatusError) -> Self {
        Self::new(error.code)
            .title(error.name.clone())
            .detail(error.brief.clone())
    }
}

impl From<StatusError> for Problem {
    fn from(error: StatusError) -> Self {
        Self::from(&error)
    }
}

impl Display for Problem {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}: {}", self.status.as_u16(), self.title, self.kind)?;
        if let Some(detail) = &self.detail {
            write!(formatter, ": {detail}")?;
        }
        Ok(())
    }
}

impl StdError for Problem {}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::http::ResBody;

    #[test]
    fn serializes_standard_and_extension_members() {
        let problem = Problem::new(StatusCode::UNPROCESSABLE_ENTITY)
            .kind("https://example.com/problems/validation-error")
            .title("The request is not valid")
            .detail("The age field must be a positive integer")
            .instance("/problems/123")
            .extension("errors", json!([{"pointer": "#/age"}]));

        assert_eq!(
            serde_json::to_value(problem).expect("problem should serialize"),
            json!({
                "type": "https://example.com/problems/validation-error",
                "title": "The request is not valid",
                "status": 422,
                "detail": "The age field must be a positive integer",
                "instance": "/problems/123",
                "errors": [{"pointer": "#/age"}]
            })
        );
    }

    #[test]
    fn defaults_to_about_blank() {
        assert_eq!(
            serde_json::to_value(Problem::new(StatusCode::NOT_FOUND))
                .expect("problem should serialize"),
            json!({
                "type": "about:blank",
                "title": "Not Found",
                "status": 404
            })
        );
    }

    #[test]
    fn standard_member_names_cannot_be_extensions() {
        let problem = Problem::new(StatusCode::BAD_REQUEST)
            .extension("type", json!("https://example.com/problems/replaced"))
            .extension("status", json!(500));

        assert!(problem.extensions().is_empty());
        assert_eq!(
            serde_json::to_value(problem).expect("problem should serialize"),
            json!({
                "type": "about:blank",
                "title": "Bad Request",
                "status": 400
            })
        );
    }

    #[test]
    fn render_sets_status_content_type_and_body() {
        let mut response = Response::new();
        response.render(Problem::new(StatusCode::CONFLICT).detail("conflicting state"));

        assert_eq!(response.status_code, Some(StatusCode::CONFLICT));
        assert_eq!(
            response.headers().get(CONTENT_TYPE),
            Some(&HeaderValue::from_static(PROBLEM_JSON))
        );
        let ResBody::Once(body) = response.take_body() else {
            panic!("expected a single problem details body");
        };
        assert_eq!(
            serde_json::from_slice::<Value>(&body).expect("body should be JSON"),
            json!({
                "type": "about:blank",
                "title": "Conflict",
                "status": 409,
                "detail": "conflicting state"
            })
        );
    }

    #[test]
    fn status_error_conversion_does_not_expose_internal_details() {
        let status_error = StatusError::internal_server_error()
            .brief("request failed")
            .detail("database password was rejected")
            .cause(std::io::Error::other("internal failure"));

        let value = serde_json::to_value(Problem::from(status_error))
            .expect("problem should serialize");
        assert_eq!(value["detail"], "request failed");
        assert!(!value.to_string().contains("database password"));
        assert!(!value.to_string().contains("internal failure"));
    }
}
