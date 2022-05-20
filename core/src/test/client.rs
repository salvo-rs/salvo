use std::convert::TryInto;
use std::time::Duration;

use http::header::{HeaderValue, IntoHeaderName};
use http::Method;

use super::request::{header_append, header_insert, BaseSettings, RequestBuilder};
use crate::error::{Error, Result};

/// `TestClient` is a type that can carry settings over multiple requests. The settings applied to the
/// `TestClient` are applied to every request created from this `TestClient`.
#[derive(Debug, Default)]
pub struct TestClient;

impl TestClient {
    /// Create a new `RequestBuilder` with the GET method and this TestClient's settings applied on it.
    pub fn get<U>(url: U) -> RequestBuilder
    where
        U: AsRef<str>,
    {
        RequestBuilder::new(url, Method::GET)
    }

    /// Create a new `RequestBuilder` with the POST method and this TestClient's settings applied on it.
    pub fn post<U>(url: U) -> RequestBuilder
    where
        U: AsRef<str>,
    {
        RequestBuilder::new(url, Method::POST)
    }

    /// Create a new `RequestBuilder` with the PUT method and this TestClient's settings applied on it.
    pub fn put<U>(url: U) -> RequestBuilder
    where
        U: AsRef<str>,
    {
        RequestBuilder::new(url, Method::PUT)
    }

    /// Create a new `RequestBuilder` with the DELETE method and this TestClient's settings applied on it.
    pub fn delete<U>(url: U) -> RequestBuilder
    where
        U: AsRef<str>,
    {
        RequestBuilder::new(url, Method::DELETE)
    }

    /// Create a new `RequestBuilder` with the HEAD method and this TestClient's settings applied on it.
    pub fn head<U>(url: U) -> RequestBuilder
    where
        U: AsRef<str>,
    {
        RequestBuilder::new(url, Method::HEAD)
    }

    /// Create a new `RequestBuilder` with the OPTIONS method and this TestClient's settings applied on it.
    pub fn options<U>(url: U) -> RequestBuilder
    where
        U: AsRef<str>,
    {
        RequestBuilder::new(url, Method::OPTIONS)
    }

    /// Create a new `RequestBuilder` with the PATCH method and this TestClient's settings applied on it.
    pub fn patch<U>(url: U) -> RequestBuilder
    where
        U: AsRef<str>,
    {
        RequestBuilder::new(url, Method::PATCH)
    }

    /// Create a new `RequestBuilder` with the TRACE method and this TestClient's settings applied on it.
    pub fn trace<U>(url: U) -> RequestBuilder
    where
        U: AsRef<str>,
    {
        RequestBuilder::new(url, Method::TRACE)
    }
}
