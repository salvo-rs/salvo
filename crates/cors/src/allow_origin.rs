use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;

use salvo_core::http::header::{self, HeaderName, HeaderValue};
use salvo_core::{Depot, Request};

use super::{Any, WILDCARD};

/// Holds configuration for how to set the [`Access-Control-Allow-Origin`][mdn] header.
///
/// See [`Cors::allow_origin`] for more details.
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Allow-Origin
/// [`Cors::allow_origin`]: super::Cors::allow_origin
#[derive(Clone, Default)]
#[must_use]
pub struct AllowOrigin(OriginInner);

type JudgeFn =
    Arc<dyn for<'a> Fn(&'a HeaderValue, &'a Request, &'a Depot) -> bool + Send + Sync + 'static>;
impl AllowOrigin {
    /// Allow any origin by sending a wildcard (`*`)
    ///
    /// See [`Cors::allow_origin`] for more details.
    ///
    /// [`Cors::allow_origin`]: super::Cors::allow_origin
    pub fn any() -> Self {
        Self(OriginInner::Exact(WILDCARD.clone()))
    }

    /// Set a single allowed origin
    ///
    /// See [`Cors::allow_origin`] for more details.
    ///
    /// [`Cors::allow_origin`]: super::Cors::allow_origin
    pub fn exact(origin: HeaderValue) -> Self {
        Self(OriginInner::Exact(origin))
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
        if origins.iter().any(|o| o == WILDCARD) {
            panic!(
                "Wildcard origin (`*`) cannot be passed to `AllowOrigin::list`. Use `AllowOrigin::any()` instead"
            );
        } else {
            Self(OriginInner::List(origins))
        }
    }

    /// Set the allowed origins from a predicate
    ///
    /// See [`Cors::allow_origin`] for more details.
    ///
    /// [`Cors::allow_origin`]: super::Cors::allow_origin
    pub fn judge<F>(f: F) -> Self
    where
        F: Fn(&HeaderValue, &Request, &Depot) -> bool + Send + Sync + 'static,
    {
        Self(OriginInner::Judge(Arc::new(f)))
    }

    /// Allow any origin, by mirroring the request origin.
    ///
    /// See [`Cors::allow_origin`] for more details.
    ///
    /// [`Cors::allow_origin`]: super::Cors::allow_origin
    pub fn mirror_request() -> Self {
        Self::judge(|_, _, _| true)
    }

    pub(super) fn is_wildcard(&self) -> bool {
        matches!(&self.0, OriginInner::Exact(v) if v == WILDCARD)
    }

    pub(super) fn to_header(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> Option<(HeaderName, HeaderValue)> {
        let allow_origin = match &self.0 {
            OriginInner::Exact(v) => v.clone(),
            OriginInner::List(l) => origin.filter(|o| l.contains(o))?.clone(),
            OriginInner::Judge(c) => origin.filter(|origin| c(origin, req, depot))?.clone(),
        };

        Some((header::ACCESS_CONTROL_ALLOW_ORIGIN, allow_origin))
    }
}

impl Debug for AllowOrigin {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.0 {
            OriginInner::Exact(inner) => f.debug_tuple("Exact").field(inner).finish(),
            OriginInner::List(inner) => f.debug_tuple("List").field(inner).finish(),
            OriginInner::Judge(_) => f.debug_tuple("Judge").finish(),
        }
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

#[derive(Clone)]
enum OriginInner {
    Exact(HeaderValue),
    List(Vec<HeaderValue>),
    Judge(JudgeFn),
}

impl Default for OriginInner {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}
