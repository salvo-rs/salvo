use std::fmt::{self, Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;

use super::{Any, WILDCARD, separated_by_commas};
use salvo_core::http::header::{self, HeaderName, HeaderValue};
use salvo_core::{Depot, Request};

/// Holds configuration for how to set the [`Access-Control-Allow-Headers`][mdn] header.
///
/// See [`Cors::allow_headers`] for more details.
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Allow-Headers
/// [`Cors::allow_headers`]: super::Cors::allow_headers
#[derive(Clone, Default)]
#[must_use]
pub struct AllowHeaders(AllowHeadersInner);

impl AllowHeaders {
    /// Allow any headers by sending a wildcard (`*`)
    ///
    /// See [`Cors::allow_headers`] for more details.
    ///
    /// [`Cors::allow_headers`]: super::Cors::allow_headers
    pub fn any() -> Self {
        Self(AllowHeadersInner::Exact(WILDCARD.clone()))
    }

    /// Set multiple allowed headers
    ///
    /// See [`Cors::allow_headers`] for more details.
    ///
    /// [`Cors::allow_headers`]: super::Cors::allow_headers
    pub fn list<I>(headers: I) -> Self
    where
        I: IntoIterator<Item = HeaderName>,
    {
        let headers = headers.into_iter().map(Into::into);
        match separated_by_commas(headers) {
            None => Self(AllowHeadersInner::None),
            Some(v) => Self(AllowHeadersInner::Exact(v)),
        }
    }

    /// Set allow headers by a closure
    ///
    /// See [`Cors::allow_headers`] for more details.
    ///
    /// [`Cors::allow_headers`]: super::Cors::allow_headers
    pub fn dynamic<C>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Option<HeaderValue>
            + Send
            + Sync
            + 'static,
    {
        Self(AllowHeadersInner::Dynamic(Arc::new(c)))
    }

    /// Set allow headers by a async closure
    ///
    /// See [`Cors::allow_headers`] for more details.
    ///
    /// [`Cors::allow_headers`]: super::Cors::allow_headers
    pub fn dynamic_async<C, Fut>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<HeaderValue>> + Send + 'static,
    {
        Self(AllowHeadersInner::DynamicAsync(Arc::new(
            move |header, req, depot| Box::pin(c(header, req, depot)),
        )))
    }

    /// Allow any headers, by mirroring the preflight [`Access-Control-Request-Headers`][mdn]
    /// header.
    ///
    /// See [`Cors::allow_headers`] for more details.
    ///
    /// [`Cors::allow_headers`]: super::Cors::allow_headers
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Request-Headers
    pub fn mirror_request() -> Self {
        Self(AllowHeadersInner::MirrorRequest)
    }

    pub(super) fn is_wildcard(&self) -> bool {
        matches!(&self.0, AllowHeadersInner::Exact(v) if v == WILDCARD)
    }

    pub(super) async fn to_header(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> Option<(HeaderName, HeaderValue)> {
        let allow_headers = match &self.0 {
            AllowHeadersInner::None => return None,
            AllowHeadersInner::Exact(v) => v.clone(),
            AllowHeadersInner::MirrorRequest => req
                .headers()
                .get(header::ACCESS_CONTROL_REQUEST_HEADERS)?
                .clone(),
            AllowHeadersInner::Dynamic(d) => d(origin, req, depot)?,
            AllowHeadersInner::DynamicAsync(d) => d(origin, req, depot).await?,
        };

        Some((header::ACCESS_CONTROL_ALLOW_HEADERS, allow_headers))
    }
}

impl Debug for AllowHeaders {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.0 {
            AllowHeadersInner::None => f.debug_tuple("None").finish(),
            AllowHeadersInner::Exact(inner) => f.debug_tuple("Exact").field(inner).finish(),
            AllowHeadersInner::MirrorRequest => f.debug_tuple("MirrorRequest").finish(),
            AllowHeadersInner::Dynamic(_) => f.debug_tuple("Dynamic").finish(),
            AllowHeadersInner::DynamicAsync(_) => f.debug_tuple("DynamicAsync").finish(),
        }
    }
}

impl From<Any> for AllowHeaders {
    fn from(_: Any) -> Self {
        Self::any()
    }
}

impl<const N: usize> From<[HeaderName; N]> for AllowHeaders {
    fn from(arr: [HeaderName; N]) -> Self {
        Self::list(arr)
    }
}

impl From<Vec<HeaderName>> for AllowHeaders {
    fn from(vec: Vec<HeaderName>) -> Self {
        Self::list(vec)
    }
}

impl From<&str> for AllowHeaders {
    fn from(val: &str) -> Self {
        Self::list([HeaderName::from_str(val).expect("Invalid header name.")])
    }
}

impl From<&String> for AllowHeaders {
    fn from(val: &String) -> Self {
        Self::list([HeaderName::from_str(val).expect("Invalid header name.")])
    }
}

impl From<Vec<&str>> for AllowHeaders {
    fn from(vals: Vec<&str>) -> Self {
        Self::list(
            vals.into_iter()
                .map(|v| HeaderName::from_str(v).expect("Invalid header name."))
                .collect::<Vec<_>>(),
        )
    }
}
impl From<&Vec<String>> for AllowHeaders {
    fn from(vals: &Vec<String>) -> Self {
        Self::list(
            vals.iter()
                .map(|v| HeaderName::from_str(v).expect("Invalid header name."))
                .collect::<Vec<_>>(),
        )
    }
}

#[derive(Clone)]
enum AllowHeadersInner {
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

impl Default for AllowHeadersInner {
    fn default() -> Self {
        Self::None
    }
}
#[cfg(test)]
mod tests {
    use salvo_core::http::header;

    use super::{AllowHeaders, AllowHeadersInner, Any};

    #[test]
    fn test_from_any() {
        let headers: AllowHeaders = Any.into();
        assert!(matches!(headers.0, AllowHeadersInner::Exact(ref v) if v == "*"));
    }

    #[test]
    fn test_from_list() {
        let headers: AllowHeaders = vec![header::CONTENT_TYPE, header::ACCEPT].into();
        assert!(matches!(headers.0, AllowHeadersInner::Exact(ref v) if v == "content-type,accept"));
    }
}
