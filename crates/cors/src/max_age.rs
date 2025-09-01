use std::fmt::{self, Debug, Formatter};
use std::pin::Pin;
use std::{sync::Arc, time::Duration};

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

#[derive(Clone)]
enum MaxAgeInner {
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

impl Default for MaxAgeInner {
    fn default() -> Self {
        Self::None
    }
}
