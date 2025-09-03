use std::fmt::{self, Debug, Formatter};
use std::pin::Pin;
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

    /// Allow custom headers by a closure
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
        Self(ExposeHeadersInner::Dynamic(Arc::new(c)))
    }

    /// Allow custom headers by a async closure
    ///
    /// See [`Cors::allow_headers`] for more details.
    ///
    /// [`Cors::allow_headers`]: super::Cors::allow_headers
    pub fn dynamic_async<C, Fut>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<HeaderValue>> + Send + 'static,
    {
        Self(ExposeHeadersInner::DynamicAsync(Arc::new(
            move |header, req, depot| Box::pin(c(header, req, depot)),
        )))
    }

    pub(super) fn is_wildcard(&self) -> bool {
        matches!(&self.0, ExposeHeadersInner::Exact(v) if v == WILDCARD)
    }

    pub(super) async fn to_header(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> Option<(HeaderName, HeaderValue)> {
        let expose_headers = match &self.0 {
            ExposeHeadersInner::None => return None,
            ExposeHeadersInner::Exact(v) => v.clone(),
            ExposeHeadersInner::Dynamic(c) => c(origin, req, depot)?,
            ExposeHeadersInner::DynamicAsync(c) => c(origin, req, depot).await?,
        };

        Some((header::ACCESS_CONTROL_EXPOSE_HEADERS, expose_headers))
    }
}

impl Debug for ExposeHeaders {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ExposeHeadersInner::None => f.debug_tuple("None").finish(),
            ExposeHeadersInner::Exact(inner) => f.debug_tuple("Exact").field(inner).finish(),
            ExposeHeadersInner::Dynamic(_) => f.debug_tuple("Dynamic").finish(),
            ExposeHeadersInner::DynamicAsync(_) => f.debug_tuple("DynamicAsync").finish(),
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
    use salvo_core::http::header::{self, HeaderValue};
    use salvo_core::{Depot, Request};

    use super::{Any, ExposeHeaders, ExposeHeadersInner};

    #[test]
    fn test_from_any() {
        let headers: ExposeHeaders = Any.into();
        assert!(matches!(headers.0, ExposeHeadersInner::Exact(ref v) if v == "*"));
    }

    #[test]
    fn test_from_list() {
        let headers: ExposeHeaders = vec![header::CONTENT_TYPE, header::ACCEPT].into();
        assert!(
            matches!(headers.0, ExposeHeadersInner::Exact(ref v) if v == "content-type,accept")
        );
    }

    #[tokio::test]
    async fn test_to_header() {
        let req = Request::default();
        let depot = Depot::new();
        let origin = HeaderValue::from_static("https://example.com");

        // Test `Any`
        let headers = ExposeHeaders::any();
        let header = headers.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((
                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                HeaderValue::from_static("*")
            ))
        );

        // Test `List`
        let headers: ExposeHeaders = vec![header::CONTENT_TYPE, header::ACCEPT].into();
        let header = headers.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((
                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                HeaderValue::from_static("content-type,accept")
            ))
        );

        // Test `Dynamic`
        let headers = ExposeHeaders::dynamic(|_, _, _| Some(HeaderValue::from_static("x-dynamic")));
        let header = headers.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((
                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                HeaderValue::from_static("x-dynamic")
            ))
        );

        // Test `DynamicAsync`
        let headers = ExposeHeaders::dynamic_async(|_, _, _| async {
            Some(HeaderValue::from_static("x-dynamic-async"))
        });
        let header = headers.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((
                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                HeaderValue::from_static("x-dynamic-async")
            ))
        );
    }
}
