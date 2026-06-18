use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;

use salvo_core::http::header::{self, HeaderName, HeaderValue};
use salvo_core::{Depot, Request};

use super::inner::HeaderInner;
use super::{Any, WILDCARD, separated_by_commas};

/// Holds configuration for how to set the [`Access-Control-Allow-Headers`][mdn] header.
///
/// See [`Cors::allow_headers`] for more details.
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Allow-Headers
/// [`Cors::allow_headers`]: super::Cors::allow_headers
#[derive(Clone, Default, Debug)]
#[must_use]
pub struct AllowHeaders(HeaderInner);

impl AllowHeaders {
    /// Allow any headers by sending a wildcard (`*`)
    ///
    /// See [`Cors::allow_headers`] for more details.
    ///
    /// [`Cors::allow_headers`]: super::Cors::allow_headers
    pub fn any() -> Self {
        Self(HeaderInner::Exact(WILDCARD.clone()))
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
            None => Self(HeaderInner::None),
            Some(v) => Self(HeaderInner::Exact(v)),
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
        Self(HeaderInner::Dynamic(Arc::new(c)))
    }

    /// Set allowed headers by an async closure.
    ///
    /// See [`Cors::allow_headers`] for more details.
    ///
    /// [`Cors::allow_headers`]: super::Cors::allow_headers
    pub fn dynamic_async<C, Fut>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<HeaderValue>> + Send + 'static,
    {
        Self(HeaderInner::DynamicAsync(Arc::new(
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
            .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
            .cloned();
        let value = self.0.resolve(origin, req, depot, mirror).await?;
        Some((header::ACCESS_CONTROL_ALLOW_HEADERS, value))
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

#[cfg(test)]
mod tests {
    use salvo_core::http::header::{self, HeaderValue};
    use salvo_core::{Depot, Request};

    use super::super::inner::HeaderInner;
    use super::{AllowHeaders, Any};

    #[test]
    fn test_from_any() {
        let headers: AllowHeaders = Any.into();
        assert!(matches!(headers.0, HeaderInner::Exact(ref v) if v == "*"));
    }

    #[test]
    fn test_from_list() {
        let headers: AllowHeaders = vec![header::CONTENT_TYPE, header::ACCEPT].into();
        assert!(matches!(headers.0, HeaderInner::Exact(ref v) if v == "content-type,accept"));
    }

    #[tokio::test]
    async fn exact_and_dynamic_do_not_require_request_headers() {
        let req = Request::default();
        let depot = Depot::new();
        let origin = HeaderValue::from_static("https://example.com");

        let headers: AllowHeaders = vec![header::CONTENT_TYPE].into();
        let header = headers.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("content-type")
            ))
        );

        let headers = AllowHeaders::dynamic(|_, _, _| Some(HeaderValue::from_static("x-dynamic")));
        let header = headers.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("x-dynamic")
            ))
        );
    }
}
