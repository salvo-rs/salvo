use std::{fmt, sync::Arc, time::Duration};

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

type JudgeFn = Arc<dyn for<'a> Fn(&'a HeaderValue, &'a Request, &'a Depot) -> HeaderValue + Send + Sync + 'static>;

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

    /// Set the max-age based on the preflight request parts
    ///
    /// See [`Cors::max_age`][super::Cors::max_age] for more details.
    pub fn judge<F>(f: F) -> Self
    where
        F: Fn(&HeaderValue, &Request, &Depot) -> HeaderValue + Send + Sync + 'static,
    {
        Self(MaxAgeInner::Judge(Arc::new(f)))
    }

    pub(super) fn to_header(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> Option<(HeaderName, HeaderValue)> {
        let max_age = match &self.0 {
            MaxAgeInner::None => return None,
            MaxAgeInner::Exact(v) => v.clone(),
            MaxAgeInner::Judge(f) => f(origin?, req, depot),
        };

        Some((header::ACCESS_CONTROL_MAX_AGE, max_age))
    }
}

impl fmt::Debug for MaxAge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            MaxAgeInner::None => f.debug_tuple("None").finish(),
            MaxAgeInner::Exact(inner) => f.debug_tuple("Exact").field(inner).finish(),
            MaxAgeInner::Judge(_) => f.debug_tuple("Judge").finish(),
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
    Judge(JudgeFn),
}

impl Default for MaxAgeInner {
    fn default() -> Self {
        Self::None
    }
}
