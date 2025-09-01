use std::fmt::{self, Debug, Formatter};
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

    /// Allow credentials for some requests, based on a given predicate
    ///
    /// See [`Cors::allow_credentials`] for more details.
    ///
    /// [`Cors::allow_credentials`]: super::Cors::allow_credentials
    pub fn predicate<P, Fut>(p: P) -> Self
    where
        P: Fn(&HeaderValue, &Request, &Depot) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = bool> + Send + 'static,
    {
        Self(AllowCredentialsInner::Predicate(Arc::new(
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
            AllowCredentialsInner::Predicate(p) => p(origin?, req, depot).await,
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
            AllowCredentialsInner::Predicate(_) => f.debug_tuple("Predicate").finish(),
        }
    }
}

#[derive(Default, Clone)]
enum AllowCredentialsInner {
    Yes,
    #[default]
    No,
    Predicate(
        Arc<
            dyn Fn(&HeaderValue, &Request, &Depot) -> Pin<Box<dyn Future<Output = bool> + Send>>
                + Send
                + Sync,
        >,
    ),
}
