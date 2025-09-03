use std::fmt::{self, Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use salvo_core::http::Method;
use salvo_core::http::header::{self, HeaderName, HeaderValue};
use salvo_core::{Depot, Request};

use super::{Any, WILDCARD, separated_by_commas};

/// Holds configuration for how to set the [`Access-Control-Allow-Methods`][mdn] header.
///
/// See [`Cors::allow_methods`] for more details.
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Allow-Methods
/// [`Cors::allow_methods`]: super::Cors::allow_methods
#[derive(Clone, Default)]
#[must_use]
pub struct AllowMethods(AllowMethodsInner);

impl AllowMethods {
    /// Allow any method by sending a wildcard (`*`)
    ///
    /// See [`Cors::allow_methods`] for more details.
    ///
    /// [`Cors::allow_methods`]: super::Cors::allow_methods
    pub fn any() -> Self {
        Self(AllowMethodsInner::Exact(WILDCARD.clone()))
    }

    /// Set a single allowed method
    ///
    /// See [`Cors::allow_methods`] for more details.
    ///
    /// [`Cors::allow_methods`]: super::Cors::allow_methods
    pub fn exact(method: &Method) -> Self {
        let value = HeaderValue::from_str(method.as_str()).expect("Invalid method.");
        Self(AllowMethodsInner::Exact(value))
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
            None => Self(AllowMethodsInner::None),
            Some(v) => Self(AllowMethodsInner::Exact(v)),
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
        Self(AllowMethodsInner::Dynamic(Arc::new(c)))
    }

    /// Set allow methods by a async closure
    ///
    /// See [`Cors::allow_methods`] for more details.
    ///
    /// [`Cors::allow_methods`]: super::Cors::allow_methods
    pub fn dynamic_async<C, Fut>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<HeaderValue>> + Send + 'static,
    {
        Self(AllowMethodsInner::DynamicAsync(Arc::new(
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
        Self(AllowMethodsInner::MirrorRequest)
    }

    pub(super) fn is_wildcard(&self) -> bool {
        matches!(&self.0, AllowMethodsInner::Exact(v) if v == WILDCARD)
    }

    pub(super) async fn to_header(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> Option<(HeaderName, HeaderValue)> {
        let allow_methods = match &self.0 {
            AllowMethodsInner::None => return None,
            AllowMethodsInner::Exact(v) => v.clone(),
            AllowMethodsInner::MirrorRequest => req
                .headers()
                .get(header::ACCESS_CONTROL_REQUEST_METHOD)?
                .clone(),
            AllowMethodsInner::Dynamic(c) => c(origin, req, depot)?,
            AllowMethodsInner::DynamicAsync(c) => c(origin, req, depot).await?,
        };

        Some((header::ACCESS_CONTROL_ALLOW_METHODS, allow_methods))
    }
}

impl Debug for AllowMethods {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.0 {
            AllowMethodsInner::None => f.debug_tuple("None").finish(),
            AllowMethodsInner::Exact(inner) => f.debug_tuple("Exact").field(inner).finish(),
            AllowMethodsInner::MirrorRequest => f.debug_tuple("MirrorRequest").finish(),
            AllowMethodsInner::Dynamic(_) => f.debug_tuple("Predicate").finish(),
            AllowMethodsInner::DynamicAsync(_) => f.debug_tuple("DynamicAsync").finish(),
        }
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

#[derive(Default, Clone)]
enum AllowMethodsInner {
    #[default]
    None,
    Exact(HeaderValue),
    MirrorRequest,
    Dynamic(
        Arc<dyn Fn(Option<&HeaderValue>, &Request, &Depot) -> Option<HeaderValue> + Send + Sync>,
    ),
    DynamicAsync(
        Arc<
            dyn Fn(
                    Option<&HeaderValue>,
                    &Request,
                    &Depot,
                ) -> Pin<Box<dyn Future<Output = Option<HeaderValue>> + Send>>
                + Send
                + Sync,
        >,
    ),
}
#[cfg(test)]
mod tests {
    use salvo_core::http::Method;

    use super::{AllowMethods, AllowMethodsInner, Any};

    #[test]
    fn test_from_any() {
        let methods: AllowMethods = Any.into();
        assert!(matches!(methods.0, AllowMethodsInner::Exact(ref v) if v == "*"));
    }

    #[test]
    fn test_from_list() {
        let methods: AllowMethods = vec![Method::GET, Method::POST].into();
        assert!(matches!(methods.0, AllowMethodsInner::Exact(ref v) if v == "GET,POST"));
    }
}
