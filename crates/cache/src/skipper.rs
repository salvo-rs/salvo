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
    #[must_use]
    pub fn new() -> Self {
        Self {
            skipped_methods: HashSet::new(),
        }
    }
    /// Add the [`Method::GET`] method to skipped methods.
    #[must_use]
    pub fn skip_get(self, value: bool) -> Self {
        self.skip_method(Method::GET, value)
    }
    /// Add the [`Method::POST`] method to skipped methods.
    #[must_use]
    pub fn skip_post(self, value: bool) -> Self {
        self.skip_method(Method::POST, value)
    }
    /// Add the [`Method::PUT`] method to skipped methods.
    #[must_use]
    pub fn skip_put(self, value: bool) -> Self {
        self.skip_method(Method::PUT, value)
    }
    /// Add the [`Method::DELETE`] method to skipped methods.
    #[must_use]
    pub fn skip_delete(self, value: bool) -> Self {
        self.skip_method(Method::DELETE, value)
    }
    /// Add the [`Method::HEAD`] method to skipped methods.
    #[must_use]
    pub fn skip_head(self, value: bool) -> Self {
        self.skip_method(Method::HEAD, value)
    }
    /// Add the [`Method::PATCH`] method to skipped methods.
    #[must_use]
    pub fn skip_patch(self, value: bool) -> Self {
        self.skip_method(Method::PATCH, value)
    }
    /// Add the [`Method::OPTIONS`] method to skipped methods.
    #[must_use]
    pub fn skip_options(self, value: bool) -> Self {
        self.skip_method(Method::OPTIONS, value)
    }
    /// Add the [`Method::CONNECT`] method to skipped methods.
    #[must_use]
    pub fn skip_connect(self, value: bool) -> Self {
        self.skip_method(Method::CONNECT, value)
    }
    /// Add the [`Method::TRACE`] method to skipped methods.
    #[must_use]
    pub fn skip_trace(self, value: bool) -> Self {
        self.skip_method(Method::TRACE, value)
    }
    /// Add a [`Method`] to skipped methods.
    #[must_use]
    pub fn skip_method(mut self, method: Method, value: bool) -> Self {
        if value {
            self.skipped_methods.insert(method);
        } else {
            self.skipped_methods.remove(&method);
        }
        self
    }
    /// Add all methods to skipped methods.
    #[must_use]
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

#[cfg(test)]
mod tests {
    use super::*;
    use salvo_core::http::Method;

    #[test]
    fn test_method_skipper_new() {
        let skipper = MethodSkipper::new();
        assert!(skipper.skipped_methods.is_empty());
    }

    #[test]
    fn test_method_skipper_default() {
        let skipper = MethodSkipper::default();
        assert!(skipper.skipped_methods.is_empty());
    }

    #[test]
    fn test_skip_get() {
        let skipper = MethodSkipper::new().skip_get(true);
        assert!(skipper.skipped_methods.contains(&Method::GET));

        let skipper = skipper.skip_get(false);
        assert!(!skipper.skipped_methods.contains(&Method::GET));
    }

    #[test]
    fn test_skip_post() {
        let skipper = MethodSkipper::new().skip_post(true);
        assert!(skipper.skipped_methods.contains(&Method::POST));

        let skipper = skipper.skip_post(false);
        assert!(!skipper.skipped_methods.contains(&Method::POST));
    }

    #[test]
    fn test_skip_put() {
        let skipper = MethodSkipper::new().skip_put(true);
        assert!(skipper.skipped_methods.contains(&Method::PUT));

        let skipper = skipper.skip_put(false);
        assert!(!skipper.skipped_methods.contains(&Method::PUT));
    }

    #[test]
    fn test_skip_delete() {
        let skipper = MethodSkipper::new().skip_delete(true);
        assert!(skipper.skipped_methods.contains(&Method::DELETE));

        let skipper = skipper.skip_delete(false);
        assert!(!skipper.skipped_methods.contains(&Method::DELETE));
    }

    #[test]
    fn test_skip_head() {
        let skipper = MethodSkipper::new().skip_head(true);
        assert!(skipper.skipped_methods.contains(&Method::HEAD));

        let skipper = skipper.skip_head(false);
        assert!(!skipper.skipped_methods.contains(&Method::HEAD));
    }

    #[test]
    fn test_skip_patch() {
        let skipper = MethodSkipper::new().skip_patch(true);
        assert!(skipper.skipped_methods.contains(&Method::PATCH));

        let skipper = skipper.skip_patch(false);
        assert!(!skipper.skipped_methods.contains(&Method::PATCH));
    }

    #[test]
    fn test_skip_options() {
        let skipper = MethodSkipper::new().skip_options(true);
        assert!(skipper.skipped_methods.contains(&Method::OPTIONS));

        let skipper = skipper.skip_options(false);
        assert!(!skipper.skipped_methods.contains(&Method::OPTIONS));
    }

    #[test]
    fn test_skip_connect() {
        let skipper = MethodSkipper::new().skip_connect(true);
        assert!(skipper.skipped_methods.contains(&Method::CONNECT));

        let skipper = skipper.skip_connect(false);
        assert!(!skipper.skipped_methods.contains(&Method::CONNECT));
    }

    #[test]
    fn test_skip_trace() {
        let skipper = MethodSkipper::new().skip_trace(true);
        assert!(skipper.skipped_methods.contains(&Method::TRACE));

        let skipper = skipper.skip_trace(false);
        assert!(!skipper.skipped_methods.contains(&Method::TRACE));
    }

    #[test]
    fn test_skip_all() {
        let skipper = MethodSkipper::new().skip_all();
        assert!(skipper.skipped_methods.contains(&Method::GET));
        assert!(skipper.skipped_methods.contains(&Method::POST));
        assert!(skipper.skipped_methods.contains(&Method::PUT));
        assert!(skipper.skipped_methods.contains(&Method::DELETE));
        assert!(skipper.skipped_methods.contains(&Method::HEAD));
        assert!(skipper.skipped_methods.contains(&Method::PATCH));
        assert!(skipper.skipped_methods.contains(&Method::OPTIONS));
        assert!(skipper.skipped_methods.contains(&Method::CONNECT));
        assert!(skipper.skipped_methods.contains(&Method::TRACE));
        assert_eq!(skipper.skipped_methods.len(), 9);
    }

    #[test]
    fn test_skip_method_chain() {
        let skipper = MethodSkipper::new()
            .skip_get(true)
            .skip_post(true)
            .skip_put(true);
        assert!(skipper.skipped_methods.contains(&Method::GET));
        assert!(skipper.skipped_methods.contains(&Method::POST));
        assert!(skipper.skipped_methods.contains(&Method::PUT));
        assert_eq!(skipper.skipped_methods.len(), 3);
    }

    #[test]
    fn test_skip_all_then_allow_get() {
        let skipper = MethodSkipper::new().skip_all().skip_get(false);
        assert!(!skipper.skipped_methods.contains(&Method::GET));
        assert!(skipper.skipped_methods.contains(&Method::POST));
        assert_eq!(skipper.skipped_methods.len(), 8);
    }

    #[test]
    fn test_method_skipper_debug() {
        let skipper = MethodSkipper::new().skip_get(true);
        let debug_str = format!("{:?}", skipper);
        assert!(debug_str.contains("MethodSkipper"));
        assert!(debug_str.contains("skipped_methods"));
    }

    #[test]
    fn test_method_skipper_clone() {
        let skipper = MethodSkipper::new().skip_get(true).skip_post(true);
        let cloned = skipper.clone();
        assert_eq!(skipper.skipped_methods, cloned.skipped_methods);
    }
}
