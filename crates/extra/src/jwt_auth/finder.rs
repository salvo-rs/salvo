use std::borrow::Cow;

use salvo_core::async_trait;
use salvo_core::http::header::{HeaderName, AUTHORIZATION, PROXY_AUTHORIZATION};
use salvo_core::http::{Method, Request};

use super::ALL_METHODS;

/// JwtTokenFinder
#[async_trait]
pub trait JwtTokenFinder: Send + Sync {
    /// Get token from request.
    async fn find_token(&self, req: &mut Request) -> Option<String>;
}

/// HeaderFinder
#[derive(Eq, PartialEq, Clone, Default)]
#[non_exhaustive]
pub struct HeaderFinder {
    /// Cared methods list.
    pub cared_methods: Vec<Method>,
    /// Header names.
    pub header_names: Vec<HeaderName>,
}
impl HeaderFinder {
    /// Create new `HeaderFinder`.
    #[inline]
    pub fn new() -> Self {
        Self {
            cared_methods: ALL_METHODS.clone(),
            header_names: vec![AUTHORIZATION, PROXY_AUTHORIZATION],
        }
    }

    #[doc(hidden)]
    #[inline]
    pub fn header_names_mut(&mut self) -> &mut Vec<HeaderName> {
        &mut self.header_names
    }
    #[doc(hidden)]
    #[inline]
    pub fn header_names(mut self, header_names: impl Into<Vec<HeaderName>>) -> Self {
        self.header_names = header_names.into();
        self
    }

    /// Get cared methods list mutable reference.
    #[inline]
    pub fn cared_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.cared_methods
    }
    /// Sets cared methods list and returns Self.
    #[inline]
    pub fn cared_methods(mut self, methods: Vec<Method>) -> Self {
        self.cared_methods = methods;
        self
    }
}
#[async_trait]
impl JwtTokenFinder for HeaderFinder {
    #[inline]
    async fn find_token(&self, req: &mut Request) -> Option<String> {
        if self.cared_methods.contains(req.method()) {
            for header_name in &self.header_names {
                if let Some(Ok(auth)) = req.headers().get(header_name).map(|auth| auth.to_str()) {
                    if auth.starts_with("Bearer") {
                        return auth.split_once(' ').map(|(_, token)| token.to_owned());
                    }
                }
            }
        }
        None
    }
}

/// FormFinder
#[derive(Eq, PartialEq, Clone, Default)]
#[non_exhaustive]
pub struct FormFinder {
    /// Cared methods list.
    pub cared_methods: Vec<Method>,
    /// Form field name.
    pub field_name: Cow<'static, str>,
}
impl FormFinder {
    /// Create new `FormFinder`.
    #[inline]
    pub fn new<T: Into<Cow<'static, str>>>(field_name: T) -> Self {
        Self {
            field_name: field_name.into(),
            cared_methods: ALL_METHODS.clone(),
        }
    }
    /// Get cared methods list mutable reference.
    #[inline]
    pub fn cared_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.cared_methods
    }
    /// Sets cared methods list and return Self.
    #[inline]
    pub fn cared_methods(mut self, methods: Vec<Method>) -> Self {
        self.cared_methods = methods;
        self
    }
}
#[async_trait]
impl JwtTokenFinder for FormFinder {
    #[inline]
    async fn find_token(&self, req: &mut Request) -> Option<String> {
        if self.cared_methods.contains(req.method()) {
            req.form(&self.field_name).await
        } else {
            None
        }
    }
}

/// QueryFinder
#[derive(Eq, PartialEq, Clone, Default)]
#[non_exhaustive]
pub struct QueryFinder {
    /// Cared methods list.
    pub cared_methods: Vec<Method>,
    /// Query name.
    pub query_name: Cow<'static, str>,
}
impl QueryFinder {
    /// Create new `QueryFinder`.
    #[inline]
    pub fn new<T: Into<Cow<'static, str>>>(query_name: T) -> Self {
        Self {
            query_name: query_name.into(),
            cared_methods: ALL_METHODS.clone(),
        }
    }
    /// Get cared methods list mutable reference.
    #[inline]
    pub fn cared_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.cared_methods
    }
    /// Sets cared methods list and return Self.
    #[inline]
    pub fn cared_methods(mut self, methods: Vec<Method>) -> Self {
        self.cared_methods = methods;
        self
    }
}

#[async_trait]
impl JwtTokenFinder for QueryFinder {
    #[inline]
    async fn find_token(&self, req: &mut Request) -> Option<String> {
        if self.cared_methods.contains(req.method()) {
            req.query(&self.query_name)
        } else {
            None
        }
    }
}

/// CookieFinder
#[derive(Eq, PartialEq, Clone, Default)]
#[non_exhaustive]
pub struct CookieFinder {
    /// Cared methods list.
    pub cared_methods: Vec<Method>,
    /// Cookie name.
    pub cookie_name: Cow<'static, str>,
}
impl CookieFinder {
    /// Create new `CookieFinder`.
    #[inline]
    pub fn new<T: Into<Cow<'static, str>>>(cookie_name: T) -> Self {
        Self {
            cookie_name: cookie_name.into(),
            cared_methods: ALL_METHODS.clone(),
        }
    }
    /// Get cared methods list mutable reference.
    #[inline]
    pub fn cared_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.cared_methods
    }
    /// Sets cared methods list and return Self.
    #[inline]
    pub fn cared_methods(mut self, methods: Vec<Method>) -> Self {
        self.cared_methods = methods;
        self
    }
}
#[async_trait]
impl JwtTokenFinder for CookieFinder {
    #[inline]
    async fn find_token(&self, req: &mut Request) -> Option<String> {
        if self.cared_methods.contains(req.method()) {
            req.cookie(&self.cookie_name).map(|c| c.value().to_owned())
        } else {
            None
        }
    }
}
