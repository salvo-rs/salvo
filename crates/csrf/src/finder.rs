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
    query_name: String,
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
