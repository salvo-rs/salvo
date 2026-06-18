use std::fmt::Debug;
use std::sync::Arc;

use salvo_core::http::Method;
use salvo_core::http::header::{self, HeaderName, HeaderValue};
use salvo_core::{Depot, Request};

use super::inner::HeaderInner;
use super::{Any, WILDCARD, separated_by_commas};

/// Holds configuration for how to set the [`Access-Control-Allow-Methods`][mdn] header.
///
/// See [`Cors::allow_methods`] for more details.
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Allow-Methods
/// [`Cors::allow_methods`]: super::Cors::allow_methods
#[derive(Clone, Default, Debug)]
#[must_use]
pub struct AllowMethods(HeaderInner);

impl AllowMethods {
    /// Allow any method by sending a wildcard (`*`)
    ///
    /// See [`Cors::allow_methods`] for more details.
    ///
    /// [`Cors::allow_methods`]: super::Cors::allow_methods
    pub fn any() -> Self {
        Self(HeaderInner::Exact(WILDCARD.clone()))
    }

    /// Set a single allowed method
    ///
    /// See [`Cors::allow_methods`] for more details.
    ///
    /// [`Cors::allow_methods`]: super::Cors::allow_methods
    pub fn exact(method: &Method) -> Self {
        let value = HeaderValue::from_str(method.as_str()).expect("Invalid method.");
        Self(HeaderInner::Exact(value))
    }

    /// Set multiple allowed methods
    ///
    /// See [`Cors::allow_methods`] for more details.
    ///
    /// [`Cors::allow_methods`]: super::Cors::allow_methods
    pub fn list<I>(methods: I) -> Self
    where
        I: IntoIterator<Item = Method>,
    {
        let methods = methods
            .into_iter()
            .map(|m| HeaderValue::from_str(m.as_str()).expect("Invalid method."));
        match separated_by_commas(methods) {
            None => Self(HeaderInner::None),
            Some(v) => Self(HeaderInner::Exact(v)),
        }
    }

    /// Set allow methods by a closure
    ///
    /// See [`Cors::allow_methods`] for more details.
    ///
    /// [`Cors::allow_methods`]: super::Cors::allow_methods
    pub fn dynamic<C>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Option<HeaderValue>
            + Send
            + Sync
            + 'static,
    {
        Self(HeaderInner::Dynamic(Arc::new(c)))
    }

    /// Set allowed methods by an async closure.
    ///
    /// See [`Cors::allow_methods`] for more details.
    ///
    /// [`Cors::allow_methods`]: super::Cors::allow_methods
    pub fn dynamic_async<C, Fut>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<HeaderValue>> + Send + 'static,
    {
        Self(HeaderInner::DynamicAsync(Arc::new(
            move |header, req, depot| Box::pin(c(header, req, depot)),
        )))
    }

    /// Allow any method, by mirroring the preflight [`Access-Control-Request-Method`][mdn]
    /// header.
    ///
    /// See [`Cors::allow_methods`] for more details.
    ///
    /// [`Cors::allow_methods`]: super::Cors::allow_methods
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Request-Method
    pub fn mirror_request() -> Self {
        Self(HeaderInner::MirrorRequest)
    }

    pub(super) fn is_wildcard(&self) -> bool {
        matches!(&self.0, HeaderInner::Exact(v) if v == WILDCARD)
    }

    pub(super) async fn to_header(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> Option<(HeaderName, HeaderValue)> {
        let mirror = req
            .headers()
            .get(header::ACCESS_CONTROL_REQUEST_METHOD)
            .cloned();
        let value = self.0.resolve(origin, req, depot, mirror).await?;
        Some((header::ACCESS_CONTROL_ALLOW_METHODS, value))
    }
}

impl From<Any> for AllowMethods {
    fn from(_: Any) -> Self {
        Self::any()
    }
}

impl From<Method> for AllowMethods {
    fn from(method: Method) -> Self {
        Self::exact(&method)
    }
}

impl<const N: usize> From<[Method; N]> for AllowMethods {
    fn from(arr: [Method; N]) -> Self {
        Self::list(arr)
    }
}

impl From<Vec<Method>> for AllowMethods {
    fn from(vec: Vec<Method>) -> Self {
        Self::list(vec)
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::http::header::HeaderValue;
    use salvo_core::http::{Method, header};
    use salvo_core::{Depot, Request};

    use super::{AllowMethods, Any};
    use crate::inner::HeaderInner;

    #[test]
    fn test_from_any() {
        let methods: AllowMethods = Any.into();
        assert!(matches!(methods.0, HeaderInner::Exact(ref v) if v == "*"));
    }

    #[test]
    fn test_from_list() {
        let methods: AllowMethods = vec![Method::GET, Method::POST].into();
        assert!(matches!(methods.0, HeaderInner::Exact(ref v) if v == "GET,POST"));
    }

    #[tokio::test]
    async fn exact_and_dynamic_do_not_require_request_method() {
        let req = Request::default();
        let depot = Depot::new();
        let origin = HeaderValue::from_static("https://example.com");

        let methods: AllowMethods = vec![Method::GET].into();
        let header = methods.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET")
            ))
        );

        let methods = AllowMethods::dynamic(|_, _, _| Some(HeaderValue::from_static("PATCH")));
        let header = methods.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("PATCH")
            ))
        );
    }
}
