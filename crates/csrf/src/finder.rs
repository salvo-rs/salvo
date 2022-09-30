use std::collections::HashMap;

use salvo_core::{async_trait,  Request};
use serde_json::Value;

/// CsrfTokenFinder
#[async_trait]
pub trait CsrfTokenFinder: Send + Sync + 'static {
    /// Find token from request.
    async fn find_token(&self, req: &mut Request) -> Option<String>;
}

pub struct QueryFinder {
    query_name: String,
}
impl QueryFinder {
    /// Create new `QueryFinder`.
    #[inline]
    pub fn new<T: Into<String>>(query_name: T) -> Self {
        Self {
            query_name: query_name.into(),
        }
    }
}
#[async_trait]
impl CsrfTokenFinder for QueryFinder {
    #[inline]
    async fn find_token(&self, req: &mut Request) -> Option<String> {
        req.query(&self.query_name)
    }
}

pub struct HeaderFinder {
    header_name: String,
}
impl HeaderFinder {
    /// Create new `HeaderFinder`.
    #[inline]
    pub fn new<T: Into<String>>(header_name: T) -> Self {
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

pub struct FormFinder {
    field_name: String,
}
impl FormFinder {
    /// Create new `FormFinder`.
    #[inline]
    pub fn new<T: Into<String>>(field_name: T) -> Self {
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

pub struct JsonFinder {
    field_name: String,
}
impl JsonFinder {
    /// Create new `FormFinder`.
    #[inline]
    pub fn new<T: Into<String>>(field_name: T) -> Self {
        Self {
            field_name: field_name.into(),
        }
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
