use std::fmt::{self, Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use salvo_core::http::header::{self, HeaderName, HeaderValue};
use salvo_core::{Depot, Request};

/// Holds configuration for how to set the [`Access-Control-Allow-Credentials`][mdn] header.
///
/// See [`Cors::allow_credentials`] for more details.
///
/// [mdn]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Access-Control-Allow-Credentials
/// [`Cors::allow_credentials`]: super::Cors::allow_credentials
#[derive(Clone, Default)]
#[must_use]
pub struct AllowCredentials(AllowCredentialsInner);

impl AllowCredentials {
    /// Allow credentials for all requests
    ///
    /// See [`Cors::allow_credentials`] for more details.
    ///
    /// [`Cors::allow_credentials`]: super::Cors::allow_credentials
    pub fn yes() -> Self {
        Self(AllowCredentialsInner::Yes)
    }

    /// Allow credentials for some requests by a closure
    ///
    /// See [`Cors::allow_credentials`] for more details.
    ///
    /// [`Cors::allow_credentials`]: super::Cors::allow_credentials
    pub fn dynamic<P>(p: P) -> Self
    where
        P: Fn(&HeaderValue, &Request, &Depot) -> bool + Send + Sync + 'static,
    {
        Self(AllowCredentialsInner::Dynamic(Arc::new(p)))
    }

    /// Allow credentials for some requests by a async closure
    ///
    /// See [`Cors::allow_credentials`] for more details.
    ///
    /// [`Cors::allow_credentials`]: super::Cors::allow_credentials
    pub fn dynamic_sync<P, Fut>(p: P) -> Self
    where
        P: Fn(&HeaderValue, &Request, &Depot) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = bool> + Send + 'static,
    {
        Self(AllowCredentialsInner::DynamicSync(Arc::new(
            move |header, req, depot| Box::pin(p(header, req, depot)),
        )))
    }

    pub(super) fn is_true(&self) -> bool {
        matches!(&self.0, AllowCredentialsInner::Yes)
    }

    pub(super) async fn to_header(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> Option<(HeaderName, HeaderValue)> {
        let allow_creds = match &self.0 {
            AllowCredentialsInner::Yes => true,
            AllowCredentialsInner::No => false,
            AllowCredentialsInner::Dynamic(p) => p(origin?, req, depot),
            AllowCredentialsInner::DynamicSync(p) => p(origin?, req, depot).await,
        };

        allow_creds.then_some((
            header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
            HeaderValue::from_static("true"),
        ))
    }
}

impl From<bool> for AllowCredentials {
    fn from(v: bool) -> Self {
        match v {
            true => Self(AllowCredentialsInner::Yes),
            false => Self(AllowCredentialsInner::No),
        }
    }
}

impl Debug for AllowCredentials {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.0 {
            AllowCredentialsInner::Yes => f.debug_tuple("Yes").finish(),
            AllowCredentialsInner::No => f.debug_tuple("No").finish(),
            AllowCredentialsInner::Dynamic(_) => f.debug_tuple("Dynamic").finish(),
            AllowCredentialsInner::DynamicSync(_) => f.debug_tuple("DynamicSync").finish(),
        }
    }
}

#[derive(Default, Clone)]
enum AllowCredentialsInner {
    Yes,
    #[default]
    No,
    Dynamic(Arc<dyn Fn(&HeaderValue, &Request, &Depot) -> bool + Send + Sync>),
    DynamicSync(
        Arc<
            dyn Fn(&HeaderValue, &Request, &Depot) -> Pin<Box<dyn Future<Output = bool> + Send>>
                + Send
                + Sync,
        >,
    ),
}
#[cfg(test)]
mod tests {
    use super::{AllowCredentials, AllowCredentialsInner};

    #[test]
    fn test_from_bool() {
        let creds: AllowCredentials = true.into();
        assert!(matches!(creds.0, AllowCredentialsInner::Yes));

        let creds: AllowCredentials = false.into();
        assert!(matches!(creds.0, AllowCredentialsInner::No));
    }
}
