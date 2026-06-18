use std::fmt::Debug;
use std::sync::Arc;

use salvo_core::http::header::{self, HeaderName, HeaderValue};
use salvo_core::{Depot, Request};

use super::{Any, WILDCARD};
use crate::inner::HeaderValueListInner;

/// Holds configuration for how to set the [`Access-Control-Allow-Origin`][mdn] header.
///
/// See [`Cors::allow_origin`] for more details.
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Allow-Origin
/// [`Cors::allow_origin`]: super::Cors::allow_origin
#[derive(Clone, Default, Debug)]
#[must_use]
pub struct AllowOrigin(HeaderValueListInner);

impl AllowOrigin {
    /// Allow any origin by sending a wildcard (`*`)
    ///
    /// See [`Cors::allow_origin`] for more details.
    ///
    /// [`Cors::allow_origin`]: super::Cors::allow_origin
    pub fn any() -> Self {
        Self(HeaderValueListInner::Exact(WILDCARD.clone()))
    }

    /// Set a single allowed origin
    ///
    /// See [`Cors::allow_origin`] for more details.
    ///
    /// [`Cors::allow_origin`]: super::Cors::allow_origin
    pub fn exact(origin: HeaderValue) -> Self {
        Self(HeaderValueListInner::Exact(origin))
    }

    /// Set multiple allowed origins
    ///
    /// See [`Cors::allow_origin`] for more details.
    ///
    /// # Panics
    ///
    /// Panics if the iterator contains a wildcard (`*`).
    ///
    /// [`Cors::allow_origin`]: super::Cors::allow_origin
    pub fn list<I>(origins: I) -> Self
    where
        I: IntoIterator<Item = HeaderValue>,
    {
        let origins = origins.into_iter().collect::<Vec<_>>();
        if origins.contains(&WILDCARD) {
            panic!(
                "Wildcard origin (`*`) cannot be passed to `AllowOrigin::list`. Use `AllowOrigin::any()` instead"
            );
        } else {
            Self(HeaderValueListInner::List(origins))
        }
    }

    /// Set the allowed origins by a closure
    ///
    /// See [`Cors::allow_origin`] for more details.
    ///
    /// [`Cors::allow_origin`]: super::Cors::allow_origin
    pub fn dynamic<C>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Option<HeaderValue>
            + Send
            + Sync
            + 'static,
    {
        Self(HeaderValueListInner::Dynamic(Arc::new(c)))
    }

    /// Set the allowed origins by an async closure.
    ///
    /// See [`Cors::allow_origin`] for more details.
    ///
    /// [`Cors::allow_origin`]: super::Cors::allow_origin
    pub fn dynamic_async<C, Fut>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<HeaderValue>> + Send + 'static,
    {
        Self(HeaderValueListInner::DynamicAsync(Arc::new(
            move |header, req, depot| Box::pin(c(header, req, depot)),
        )))
    }

    /// Allow any origin, by mirroring the request origin.
    ///
    /// See [`Cors::allow_origin`] for more details.
    ///
    /// [`Cors::allow_origin`]: super::Cors::allow_origin
    pub fn mirror_request() -> Self {
        Self::dynamic(|v, _, _| v.cloned())
    }

    pub(super) fn is_wildcard(&self) -> bool {
        matches!(&self.0, HeaderValueListInner::Exact(v) if v == WILDCARD)
    }

    pub(super) async fn to_header(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> Option<(HeaderName, HeaderValue)> {
        let allow_origin = match &self.0 {
            HeaderValueListInner::Exact(v) => v.clone(),
            HeaderValueListInner::List(l) => origin.filter(|o| l.contains(o))?.clone(),
            HeaderValueListInner::Dynamic(c) => c(origin, req, depot)?,
            HeaderValueListInner::DynamicAsync(c) => c(origin, req, depot).await?,
        };

        Some((header::ACCESS_CONTROL_ALLOW_ORIGIN, allow_origin))
    }
}

impl From<Any> for AllowOrigin {
    fn from(_: Any) -> Self {
        Self::any()
    }
}

impl From<HeaderValue> for AllowOrigin {
    fn from(val: HeaderValue) -> Self {
        Self::exact(val)
    }
}

impl<const N: usize> From<[HeaderValue; N]> for AllowOrigin {
    fn from(arr: [HeaderValue; N]) -> Self {
        Self::list(arr)
    }
}

impl From<Vec<HeaderValue>> for AllowOrigin {
    fn from(vec: Vec<HeaderValue>) -> Self {
        Self::list(vec)
    }
}

impl From<&str> for AllowOrigin {
    fn from(val: &str) -> Self {
        Self::exact(HeaderValue::from_str(val).expect("invalid `HeaderValue`"))
    }
}

impl From<&String> for AllowOrigin {
    fn from(val: &String) -> Self {
        Self::exact(HeaderValue::from_str(val).expect("invalid `HeaderValue`"))
    }
}

impl From<Vec<&str>> for AllowOrigin {
    fn from(vals: Vec<&str>) -> Self {
        Self::list(
            vals.iter()
                .map(|v| HeaderValue::from_str(v).expect("invalid `HeaderValue`"))
                .collect::<Vec<_>>(),
        )
    }
}
impl<const N: usize> From<[&str; N]> for AllowOrigin {
    fn from(vals: [&str; N]) -> Self {
        Self::list(
            vals.iter()
                .map(|v| HeaderValue::from_str(v).expect("invalid `HeaderValue`"))
                .collect::<Vec<_>>(),
        )
    }
}
impl From<&Vec<String>> for AllowOrigin {
    fn from(vals: &Vec<String>) -> Self {
        Self::list(
            vals.iter()
                .map(|v| HeaderValue::from_str(v).expect("invalid `HeaderValue`"))
                .collect::<Vec<_>>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::http::header::HeaderValue;

    use super::{AllowOrigin, Any, HeaderValueListInner, WILDCARD};

    #[test]
    fn test_from_any() {
        let origin: AllowOrigin = Any.into();
        assert!(matches!(origin.0, HeaderValueListInner::Exact(ref v) if v == "*"));
    }

    #[test]
    fn test_from_list() {
        let origin: AllowOrigin = vec!["https://example.com"].into();
        assert!(
            matches!(origin.0, HeaderValueListInner::List(ref v) if v == &vec![HeaderValue::from_static("https://example.com")])
        );
    }

    #[test]
    #[should_panic]
    fn test_list_with_wildcard() {
        let _: AllowOrigin = vec![WILDCARD.clone()].into();
    }
}
