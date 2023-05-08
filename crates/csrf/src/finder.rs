use std::collections::HashMap;

use salvo_core::{async_trait, Request};
use serde_json::Value;

/// CsrfTokenFinder
#[async_trait]
pub trait CsrfTokenFinder: Send + Sync + 'static {
    /// Find token from request.
    async fn find_token(&self, req: &mut Request) -> Option<String>;
}

/// Find token from http request url query string.
#[derive(Clone, Debug)]
pub struct QueryFinder {
    /// The query name for the csrf token.
    pub query_name: String,
}
impl Default for QueryFinder {
    fn default() -> Self {
        Self::new()
    }
}
impl QueryFinder {
    /// Create new `QueryFinder`.
    #[inline]
    pub fn new() -> Self {
        Self {
            query_name: "csrf-token".into(),
        }
    }

    /// Sets query name, it's query_name's default value is `csrf-token`.
    #[inline]
    pub fn query_name(mut self, name: impl Into<String>) -> Self {
        self.query_name = name.into();
        self
    }
}
#[async_trait]
impl CsrfTokenFinder for QueryFinder {
    #[inline]
    async fn find_token(&self, req: &mut Request) -> Option<String> {
        req.query(&self.query_name)
    }
}

/// Find token from http request header.
#[derive(Clone, Debug)]
pub struct HeaderFinder {
    header_name: String,
}
impl HeaderFinder {
    /// Create new `HeaderFinder`, you can use value like `x-csrf-token`.
    #[inline]
    pub fn new(header_name: impl Into<String>) -> Self {
        Self {
            header_name: header_name.into(),
        }
    }
}
#[async_trait]
impl CsrfTokenFinder for HeaderFinder {
    #[inline]
    async fn find_token(&self, req: &mut Request) -> Option<String> {
        req.header(&self.header_name)
    }
}

/// Find token from request form body.
#[derive(Clone, Debug)]
pub struct FormFinder {
    field_name: String,
}
impl FormFinder {
    /// Create new `FormFinder`.
    #[inline]
    pub fn new(field_name: impl Into<String>) -> Self {
        Self {
            field_name: field_name.into(),
        }
    }
}
#[async_trait]
impl CsrfTokenFinder for FormFinder {
    #[inline]
    async fn find_token(&self, req: &mut Request) -> Option<String> {
        req.form(&self.field_name).await
    }
}

/// Find token from request json body.
#[derive(Clone, Debug)]
pub struct JsonFinder {
    field_name: String,
}
impl JsonFinder {
    /// Create new `FormFinder`.
    #[inline]
    pub fn new(field_name: impl Into<String>) -> Self {
        Self {
            field_name: field_name.into(),
        }
    }
}
#[async_trait]
impl CsrfTokenFinder for JsonFinder {
    #[inline]
    async fn find_token(&self, req: &mut Request) -> Option<String> {
        let data = req.parse_json::<HashMap<String, Value>>().await;
        if let Ok(data) = data {
            if let Some(value) = data.get(&self.field_name) {
                if let Some(token) = value.as_str() {
                    return Some(token.to_owned());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use salvo_core::test::TestClient;

    #[tokio::test]
    async fn test_query_finder() {
        let query_finder = QueryFinder::new();
        let mut req = TestClient::get("http://test.com/?csrf-token=test_token").build();
        let token = query_finder.find_token(&mut req).await;
        assert_eq!(token, Some("test_token".to_string()));
    }

    #[tokio::test]
    async fn test_header_finder() {
        let header_finder = HeaderFinder::new("x-csrf-token");
        let mut req = TestClient::get("http://test.com")
            .add_header("x-csrf-token", "test_token", true)
            .build();
        let token = header_finder.find_token(&mut req).await;
        assert_eq!(token, Some("test_token".to_string()));
    }

    #[tokio::test]
    async fn test_form_finder() {
        let form_finder = FormFinder::new("csrf-token");
        let mut req = TestClient::get("http://test.com")
            .raw_form("csrf-token=test_token")
            .build();
        let token = form_finder.find_token(&mut req).await;
        assert_eq!(token, Some("test_token".to_string()));
    }

    #[tokio::test]
    async fn test_json_finder() {
        let json_finder = JsonFinder::new("csrf-token");
        let mut req = TestClient::get("http://test.com")
            .raw_json(r#"{"csrf-token":"test_token"}"#)
            .build();
        let token = json_finder.find_token(&mut req).await;
        assert_eq!(token, Some("test_token".to_string()));
    }
}
