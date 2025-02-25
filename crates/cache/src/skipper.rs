use std::collections::HashSet;

use salvo_core::handler::Skipper;
use salvo_core::http::Method;
use salvo_core::{Depot, Request};

/// Skipper for `Method`. You can use it to skip some methods.
///
/// If the request method is in the skip list, the request will be skipped.
#[derive(Default, Clone, Debug)]
pub struct MethodSkipper {
    skipped_methods: HashSet<Method>,
}
impl MethodSkipper {
    /// Create a new `MethodSkipper`.
    pub fn new() -> Self {
        Self {
            skipped_methods: HashSet::new(),
        }
    }
    /// Add the [`Method::GET`] method to skipped methods.
    pub fn skip_get(self, value: bool) -> Self {
        self.skip_method(Method::GET, value)
    }
    /// Add the [`Method::POST`] method to skipped methods.
    pub fn skip_post(self, value: bool) -> Self {
        self.skip_method(Method::POST, value)
    }
    /// Add the [`Method::PUT`] method to skipped methods.
    pub fn skip_put(self, value: bool) -> Self {
        self.skip_method(Method::PUT, value)
    }
    /// Add the [`Method::DELETE`] method to skipped methods.
    pub fn skip_delete(self, value: bool) -> Self {
        self.skip_method(Method::DELETE, value)
    }
    /// Add the [`Method::HEAD`] method to skipped methods.
    pub fn skip_head(self, value: bool) -> Self {
        self.skip_method(Method::HEAD, value)
    }
    /// Add the [`Method::PATCH`] method to skipped methods.
    pub fn skip_patch(self, value: bool) -> Self {
        self.skip_method(Method::PATCH, value)
    }
    /// Add the [`Method::OPTIONS`] method to skipped methods.
    pub fn skip_options(self, value: bool) -> Self {
        self.skip_method(Method::OPTIONS, value)
    }
    /// Add the [`Method::CONNECT`] method to skipped methods.
    pub fn skip_connect(self, value: bool) -> Self {
        self.skip_method(Method::CONNECT, value)
    }
    /// Add the [`Method::TRACE`] method to skipped methods.
    pub fn skip_trace(self, value: bool) -> Self {
        self.skip_method(Method::TRACE, value)
    }
    /// Add a [`Method`] to skipped methods.
    pub fn skip_method(mut self, method: Method, value: bool) -> Self {
        if value {
            self.skipped_methods.insert(method);
        } else {
            self.skipped_methods.remove(&method);
        }
        self
    }
    /// Add all methods to skipped methods.
    pub fn skip_all(mut self) -> Self {
        self.skipped_methods = [
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::HEAD,
            Method::PATCH,
            Method::OPTIONS,
            Method::CONNECT,
            Method::TRACE,
        ]
        .into_iter()
        .collect();
        self
    }
}
impl Skipper for MethodSkipper {
    fn skipped(&self, req: &mut Request, _depot: &Depot) -> bool {
        self.skipped_methods.contains(req.method())
    }
}
