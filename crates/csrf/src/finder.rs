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
pub struct QueryFinder {
    query_name: String,
}
impl QueryFinder {
    /// Create new `QueryFinder`.
    #[inline]
    pub fn new() -> Self {
        Self {
            query_name: "csrf-token".into(),
        }
    }

    /// Set query name, it's query_name's default value is `csrf-token`.
    #[inline]
    pub fn with_query_name(mut self, name: impl Into<String>) -> Self {
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
pub struct HeaderFinder {
    header_name: String,
}
impl Default for HeaderFinder {
    fn default() -> Self {
        Self::new()
    }
}
impl HeaderFinder {
    /// Create new `HeaderFinder`, it's header_name's default value is `x-csrf-token`.
    #[inline]
    pub fn new() -> Self {
        Self {
            header_name: "x-csrf-token".into(),
        }
    }

    /// Set header name, it's header_name's default value is `x-csrf-token`.
    #[inline]
    pub fn with_header_name(mut self, name: impl Into<String>) -> Self {
        self.header_name = name.into();
        self
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
pub struct FormFinder {
    field_name: String,
}
impl Default for FormFinder {
    fn default() -> Self {
        Self::new()
    }
}
impl FormFinder {
    /// Create new `FormFinder`.
    #[inline]
    pub fn new() -> Self {
        Self {
            field_name: "csrf-token".into(),
        }
    }

    /// Set field name, it's field_name's default value is `csrf-token`.
    #[inline]
    pub fn with_field_name(mut self, name: impl Into<String>) -> Self {
        self.field_name = name.into();
        self
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
pub struct JsonFinder {
    field_name: String,
}
impl Default for JsonFinder {
    fn default() -> Self {
        Self::new()
    }
}
impl JsonFinder {
    /// Create new `FormFinder`.
    #[inline]
    pub fn new() -> Self {
        Self {
            field_name: "csrf-token".into(),
        }
    }

    /// Set field name, it's field_name's default value is `csrf-token`.
    #[inline]
    pub fn with_field_name(mut self, name: impl Into<String>) -> Self {
        self.field_name = name.into();
        self
    }
}
#[async_trait]
impl CsrfTokenFinder for JsonFinder {
    #[inline]
    async fn find_token(&self, req: &mut Request) -> Option<String> {
        if let Ok(data) = req.parse_json::<HashMap<String, Value>>().await {
            if let Some(value) = data.get(&self.field_name) {
                if let Some(token) = value.as_str() {
                    return Some(token.to_owned());
                }
            }
        }
        None
    }
}
