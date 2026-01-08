use std::fmt::{self, Debug, Formatter};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use salvo_core::http::header::{self, HeaderName, HeaderValue};
use salvo_core::{Depot, Request};

/// Holds configuration for how to set the [`Access-Control-Max-Age`][mdn] header.
///
/// See [`Cors::max_age`][super::Cors::max_age] for more details.
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Max-Age
#[derive(Clone, Default)]
#[must_use]
pub struct MaxAge(MaxAgeInner);

impl MaxAge {
    /// Set a static max-age value
    ///
    /// See [`Cors::max_age`][super::Cors::max_age] for more details.
    pub fn exact(max_age: Duration) -> Self {
        Self(MaxAgeInner::Exact(max_age.as_secs().into()))
    }

    /// Set a static max-age value
    ///
    /// See [`Cors::max_age`][super::Cors::max_age] for more details.
    pub fn seconds(seconds: u64) -> Self {
        Self(MaxAgeInner::Exact(seconds.into()))
    }

    /// Set the max-age by a async closure
    ///
    /// See [`Cors::max_age`][super::Cors::max_age] for more details.
    pub fn dynamic<C>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Option<HeaderValue>
            + Send
            + Sync
            + 'static,
    {
        Self(MaxAgeInner::Dynamic(Arc::new(c)))
    }

    /// Set the max-age by a async closure
    ///
    /// See [`Cors::max_age`][super::Cors::max_age] for more details.
    pub fn dynamic_async<C, Fut>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<HeaderValue>> + Send + 'static,
    {
        Self(MaxAgeInner::DynamicAsync(Arc::new(
            move |header, req, depot| Box::pin(c(header, req, depot)),
        )))
    }

    pub(super) async fn to_header(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> Option<(HeaderName, HeaderValue)> {
        let max_age = match &self.0 {
            MaxAgeInner::None => return None,
            MaxAgeInner::Exact(v) => v.clone(),
            MaxAgeInner::Dynamic(f) => f(origin, req, depot)?,
            MaxAgeInner::DynamicAsync(f) => f(origin, req, depot).await?,
        };

        Some((header::ACCESS_CONTROL_MAX_AGE, max_age))
    }
}

impl Debug for MaxAge {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.0 {
            MaxAgeInner::None => f.debug_tuple("None").finish(),
            MaxAgeInner::Exact(inner) => f.debug_tuple("Exact").field(inner).finish(),
            MaxAgeInner::Dynamic(_) => f.debug_tuple("Dynamic").finish(),
            MaxAgeInner::DynamicAsync(_) => f.debug_tuple("DynamicAsync").finish(),
        }
    }
}

impl From<Duration> for MaxAge {
    fn from(max_age: Duration) -> Self {
        Self::exact(max_age)
    }
}

impl From<u64> for MaxAge {
    fn from(max_age: u64) -> Self {
        Self(MaxAgeInner::Exact(max_age.into()))
    }
}

impl From<u32> for MaxAge {
    fn from(max_age: u32) -> Self {
        Self(MaxAgeInner::Exact(max_age.into()))
    }
}

impl From<usize> for MaxAge {
    fn from(max_age: usize) -> Self {
        Self(MaxAgeInner::Exact(max_age.into()))
    }
}

impl From<i64> for MaxAge {
    fn from(max_age: i64) -> Self {
        Self(MaxAgeInner::Exact(max_age.into()))
    }
}

impl From<i32> for MaxAge {
    fn from(max_age: i32) -> Self {
        Self(MaxAgeInner::Exact(max_age.into()))
    }
}

impl From<isize> for MaxAge {
    fn from(max_age: isize) -> Self {
        Self(MaxAgeInner::Exact(max_age.into()))
    }
}

#[derive(Clone, Default)]
enum MaxAgeInner {
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
    use std::time::Duration;

    use salvo_core::http::header::{self, HeaderValue};
    use salvo_core::{Depot, Request};

    use super::{MaxAge, MaxAgeInner};

    #[test]
    fn test_from_duration() {
        let max_age: MaxAge = Duration::from_secs(3600).into();
        assert!(matches!(max_age.0, MaxAgeInner::Exact(ref v) if v == "3600"));
    }

    #[test]
    fn test_from_u64() {
        let max_age: MaxAge = 3600u64.into();
        assert!(matches!(max_age.0, MaxAgeInner::Exact(ref v) if v == "3600"));
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
