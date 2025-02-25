use std::fmt::{self, Debug, Formatter};
use std::str::FromStr;
use std::sync::Arc;

use salvo_core::http::header::{self, HeaderName, HeaderValue};
use salvo_core::{Depot, Request};

use super::{Any, WILDCARD, separated_by_commas};

/// Holds configuration for how to set the [`Access-Control-Expose-Headers`][mdn] header.
///
/// See [`Cors::expose_headers`] for more details.
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Expose-Headers
/// [`Cors::expose_headers`]: super::Cors::expose_headers
#[derive(Clone, Default)]
#[must_use]
pub struct ExposeHeaders(ExposeHeadersInner);

type JudgeFn = Arc<
    dyn for<'a> Fn(&'a HeaderValue, &'a Request, &'a Depot) -> HeaderValue + Send + Sync + 'static,
>;
impl ExposeHeaders {
    /// Expose any / all headers by sending a wildcard (`*`)
    ///
    /// See [`Cors::expose_headers`] for more details.
    ///
    /// [`Cors::expose_headers`]: super::Cors::expose_headers
    pub fn any() -> Self {
        Self(ExposeHeadersInner::Exact(WILDCARD.clone()))
    }

    /// Set multiple exposed header names
    ///
    /// See [`Cors::expose_headers`] for more details.
    ///
    /// [`Cors::expose_headers`]: super::Cors::expose_headers
    pub fn list<I>(headers: I) -> Self
    where
        I: IntoIterator<Item = HeaderName>,
    {
        match separated_by_commas(headers.into_iter().map(Into::into)) {
            None => Self(ExposeHeadersInner::None),
            Some(value) => Self(ExposeHeadersInner::Exact(value)),
        }
    }

    /// Allow custom headers based on a given predicate
    ///
    /// See [`Cors::allow_headers`] for more details.
    ///
    /// [`Cors::allow_headers`]: super::Cors::allow_headers
    pub fn judge<F>(f: F) -> Self
    where
        F: Fn(&HeaderValue, &Request, &Depot) -> HeaderValue + Send + Sync + 'static,
    {
        Self(ExposeHeadersInner::Judge(Arc::new(f)))
    }

    pub(super) fn is_wildcard(&self) -> bool {
        matches!(&self.0, ExposeHeadersInner::Exact(v) if v == WILDCARD)
    }

    pub(super) fn to_header(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> Option<(HeaderName, HeaderValue)> {
        let expose_headers = match &self.0 {
            ExposeHeadersInner::None => return None,
            ExposeHeadersInner::Exact(v) => v.clone(),
            ExposeHeadersInner::Judge(f) => f(origin?, req, depot),
        };

        Some((header::ACCESS_CONTROL_EXPOSE_HEADERS, expose_headers))
    }
}

impl Debug for ExposeHeaders {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ExposeHeadersInner::None => f.debug_tuple("None").finish(),
            ExposeHeadersInner::Exact(inner) => f.debug_tuple("Exact").field(inner).finish(),
            ExposeHeadersInner::Judge(_) => f.debug_tuple("Judge").finish(),
        }
    }
}

impl From<Any> for ExposeHeaders {
    fn from(_: Any) -> Self {
        Self::any()
    }
}

impl<const N: usize> From<[HeaderName; N]> for ExposeHeaders {
    fn from(arr: [HeaderName; N]) -> Self {
        Self::list(arr)
    }
}

impl From<Vec<HeaderName>> for ExposeHeaders {
    fn from(vec: Vec<HeaderName>) -> Self {
        Self::list(vec)
    }
}

impl From<&str> for ExposeHeaders {
    fn from(val: &str) -> Self {
        Self::list([HeaderName::from_str(val).expect("Invalid header name.")])
    }
}

impl From<&String> for ExposeHeaders {
    fn from(val: &String) -> Self {
        Self::list([HeaderName::from_str(val).expect("Invalid header name.")])
    }
}

impl From<Vec<&str>> for ExposeHeaders {
    fn from(vals: Vec<&str>) -> Self {
        Self::list(
            vals.into_iter()
                .map(|v| HeaderName::from_str(v).expect("Invalid header name."))
                .collect::<Vec<_>>(),
        )
    }
}
impl From<&Vec<String>> for ExposeHeaders {
    fn from(vals: &Vec<String>) -> Self {
        Self::list(
            vals.iter()
                .map(|v| HeaderName::from_str(v).expect("Invalid header name."))
                .collect::<Vec<_>>(),
        )
    }
}

#[derive(Default, Clone)]
enum ExposeHeadersInner {
    #[default]
    None,
    Exact(HeaderValue),
    Judge(JudgeFn),
}
