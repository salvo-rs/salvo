use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use salvo_core::http::header::{self, HeaderName, HeaderValue};
use salvo_core::{Depot, Request};

use super::inner::HeaderValueInner;

/// Holds configuration for how to set the [`Access-Control-Max-Age`][mdn] header.
///
/// See [`Cors::max_age`][super::Cors::max_age] for more details.
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Max-Age
#[derive(Clone, Default, Debug)]
#[must_use]
pub struct MaxAge(HeaderValueInner);

impl MaxAge {
    /// Set a static max-age value
    ///
    /// See [`Cors::max_age`][super::Cors::max_age] for more details.
    pub fn exact(max_age: Duration) -> Self {
        Self(HeaderValueInner::Exact(max_age.as_secs().into()))
    }

    /// Set a static max-age value
    ///
    /// See [`Cors::max_age`][super::Cors::max_age] for more details.
    pub fn seconds(seconds: u64) -> Self {
        Self(HeaderValueInner::Exact(seconds.into()))
    }

    /// Set the max-age by an async closure.
    ///
    /// See [`Cors::max_age`][super::Cors::max_age] for more details.
    pub fn dynamic<C>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Option<HeaderValue>
            + Send
            + Sync
            + 'static,
    {
        Self(HeaderValueInner::Dynamic(Arc::new(c)))
    }

    /// Set the max-age by an async closure.
    ///
    /// See [`Cors::max_age`][super::Cors::max_age] for more details.
    pub fn dynamic_async<C, Fut>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<HeaderValue>> + Send + 'static,
    {
        Self(HeaderValueInner::DynamicAsync(Arc::new(
            move |header, req, depot| Box::pin(c(header, req, depot)),
        )))
    }

    pub(super) async fn to_header(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> Option<(HeaderName, HeaderValue)> {
        let value = self.0.resolve(origin, req, depot).await?;
        Some((header::ACCESS_CONTROL_MAX_AGE, value))
    }
}

impl From<Duration> for MaxAge {
    fn from(max_age: Duration) -> Self {
        Self::exact(max_age)
    }
}

impl From<u64> for MaxAge {
    fn from(max_age: u64) -> Self {
        Self(HeaderValueInner::Exact(max_age.into()))
    }
}

impl From<u32> for MaxAge {
    fn from(max_age: u32) -> Self {
        Self(HeaderValueInner::Exact(max_age.into()))
    }
}

impl From<usize> for MaxAge {
    fn from(max_age: usize) -> Self {
        Self(HeaderValueInner::Exact(max_age.into()))
    }
}

impl From<i64> for MaxAge {
    fn from(max_age: i64) -> Self {
        Self(HeaderValueInner::Exact(max_age.into()))
    }
}

impl From<i32> for MaxAge {
    fn from(max_age: i32) -> Self {
        Self(HeaderValueInner::Exact(max_age.into()))
    }
}

impl From<isize> for MaxAge {
    fn from(max_age: isize) -> Self {
        Self(HeaderValueInner::Exact(max_age.into()))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use salvo_core::http::header::{self, HeaderValue};
    use salvo_core::{Depot, Request};

    use super::super::inner::HeaderValueInner;
    use super::MaxAge;

    #[test]
    fn test_from_duration() {
        let max_age: MaxAge = Duration::from_secs(3600).into();
        assert!(matches!(max_age.0, HeaderValueInner::Exact(ref v) if v == "3600"));
    }

    #[test]
    fn test_from_u64() {
        let max_age: MaxAge = 3600u64.into();
        assert!(matches!(max_age.0, HeaderValueInner::Exact(ref v) if v == "3600"));
    }

    #[tokio::test]
    async fn test_to_header() {
        let req = Request::default();
        let depot = Depot::new();
        let origin = HeaderValue::from_static("https://example.com");

        // Test `Exact`
        let max_age = MaxAge::exact(Duration::from_secs(3600));
        let header = max_age.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((
                header::ACCESS_CONTROL_MAX_AGE,
                HeaderValue::from_static("3600")
            ))
        );

        // Test `Seconds`
        let max_age = MaxAge::seconds(7200);
        let header = max_age.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((
                header::ACCESS_CONTROL_MAX_AGE,
                HeaderValue::from_static("7200")
            ))
        );

        // Test `Dynamic`
        let max_age = MaxAge::dynamic(|_, _, _| Some(HeaderValue::from_static("1800")));
        let header = max_age.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((
                header::ACCESS_CONTROL_MAX_AGE,
                HeaderValue::from_static("1800")
            ))
        );

        // Test `DynamicAsync`
        let max_age =
            MaxAge::dynamic_async(|_, _, _| async { Some(HeaderValue::from_static("900")) });
        let header = max_age.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((
                header::ACCESS_CONTROL_MAX_AGE,
                HeaderValue::from_static("900")
            ))
        );
    }
}
