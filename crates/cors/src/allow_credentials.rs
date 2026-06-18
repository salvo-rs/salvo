use std::fmt::Debug;
use std::sync::Arc;

use salvo_core::http::header::{self, HeaderName, HeaderValue};
use salvo_core::{Depot, Request};

use super::inner::BoolInner;

/// Holds configuration for how to set the [`Access-Control-Allow-Credentials`][mdn] header.
///
/// See [`Cors::allow_credentials`] for more details.
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Allow-Credentials
/// [`Cors::allow_credentials`]: super::Cors::allow_credentials
#[derive(Clone, Default, Debug)]
#[must_use]
pub struct AllowCredentials(BoolInner);

impl AllowCredentials {
    /// Allow credentials for all requests
    ///
    /// See [`Cors::allow_credentials`] for more details.
    ///
    /// [`Cors::allow_credentials`]: super::Cors::allow_credentials
    pub fn yes() -> Self {
        Self(BoolInner::Yes)
    }

    /// Allow credentials for some requests by a closure
    ///
    /// See [`Cors::allow_credentials`] for more details.
    ///
    /// [`Cors::allow_credentials`]: super::Cors::allow_credentials
    pub fn dynamic<P>(p: P) -> Self
    where
        P: Fn(Option<&HeaderValue>, &Request, &Depot) -> bool + Send + Sync + 'static,
    {
        Self(BoolInner::Dynamic(Arc::new(p)))
    }

    /// Allow credentials for some requests by an async closure
    ///
    /// See [`Cors::allow_credentials`] for more details.
    ///
    /// [`Cors::allow_credentials`]: super::Cors::allow_credentials
    pub fn dynamic_async<P, Fut>(p: P) -> Self
    where
        P: Fn(Option<&HeaderValue>, &Request, &Depot) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = bool> + Send + 'static,
    {
        Self(BoolInner::DynamicAsync(Arc::new(
            move |header, req, depot| Box::pin(p(header, req, depot)),
        )))
    }

    pub(super) fn is_true(&self) -> bool {
        matches!(&self.0, BoolInner::Yes)
    }

    pub(super) async fn to_header(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> Option<(HeaderName, HeaderValue)> {
        self.0.resolve_async(origin, req, depot).await.then_some((
            header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
            HeaderValue::from_static("true"),
        ))
    }
}

impl From<bool> for AllowCredentials {
    fn from(v: bool) -> Self {
        match v {
            true => Self(BoolInner::Yes),
            false => Self(BoolInner::No),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AllowCredentials;

    #[test]
    fn test_from_bool() {
        let creds: AllowCredentials = true.into();
        assert!(creds.is_true());

        let creds: AllowCredentials = false.into();
        assert!(!creds.is_true());
    }
}
