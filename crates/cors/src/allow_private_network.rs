use std::pin::Pin;
use std::{fmt, sync::Arc};

use salvo_core::http::header::{HeaderName, HeaderValue};
use salvo_core::{Depot, Request};

/// Holds configuration for how to set the [`Access-Control-Allow-Private-Network`][wicg] header.
///
/// See [`CorsLayer::allow_private_network`] for more details.
///
/// [wicg]: https://wicg.github.io/private-network-access/
/// [`CorsLayer::allow_private_network`]: super::CorsLayer::allow_private_network
#[derive(Clone, Default)]
#[must_use]
pub struct AllowPrivateNetwork(AllowPrivateNetworkInner);

impl AllowPrivateNetwork {
    /// Allow requests via a more private network than the one used to access the origin
    ///
    /// See [`CorsLayer::allow_private_network`] for more details.
    ///
    /// [`CorsLayer::allow_private_network`]: super::CorsLayer::allow_private_network
    pub fn yes() -> Self {
        Self(AllowPrivateNetworkInner::Yes)
    }

    /// Allow requests via private network for some requests by a closure
    ///
    /// See [`CorsLayer::allow_private_network`] for more details.
    ///
    /// [`CorsLayer::allow_private_network`]: super::CorsLayer::allow_private_network
    pub fn dynamic<C>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Option<HeaderValue>
            + Send
            + Sync
            + 'static,
    {
        Self(AllowPrivateNetworkInner::Dynamic(Arc::new(c)))
    }

    /// Allow requests via private network for some requests by a async closure
    ///
    /// See [`CorsLayer::allow_private_network`] for more details.
    ///
    /// [`CorsLayer::allow_private_network`]: super::CorsLayer::allow_private_network
    pub fn dynamic_async<C, Fut>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<HeaderValue>> + Send + 'static,
    {
        Self(AllowPrivateNetworkInner::DynamicAsync(Arc::new(
            move |header, req, depot| Box::pin(c(header, req, depot)),
        )))
    }

    #[allow(
        clippy::declare_interior_mutable_const,
        clippy::borrow_interior_mutable_const
    )]
    pub(super) async fn to_header(
        &self,
        origin: Option<&HeaderValue>,
        req: &Request,
        depot: &Depot,
    ) -> Option<(HeaderName, HeaderValue)> {
        #[allow(clippy::declare_interior_mutable_const)]
        const REQUEST_PRIVATE_NETWORK: HeaderName =
            HeaderName::from_static("access-control-request-private-network");

        #[allow(clippy::declare_interior_mutable_const)]
        const ALLOW_PRIVATE_NETWORK: HeaderName =
            HeaderName::from_static("access-control-allow-private-network");

        const TRUE: HeaderValue = HeaderValue::from_static("true");

        // Cheapest fallback: allow_private_network hasn't been set
        if let AllowPrivateNetworkInner::No = &self.0 {
            return None;
        }

        // Access-Control-Allow-Private-Network is only relevant if the request
        // has the Access-Control-Request-Private-Network header set, else skip
        if req.headers().get(REQUEST_PRIVATE_NETWORK) != Some(&TRUE) {
            return None;
        }

        match &self.0 {
            AllowPrivateNetworkInner::Yes => Some((ALLOW_PRIVATE_NETWORK, TRUE)),
            AllowPrivateNetworkInner::No => None,
            AllowPrivateNetworkInner::Dynamic(c) => {
                c(origin, req, depot).map(|v| (ALLOW_PRIVATE_NETWORK, v))
            }
            AllowPrivateNetworkInner::DynamicAsync(c) => c(origin, req, depot)
                .await
                .map(|v| (ALLOW_PRIVATE_NETWORK, v)),
        }
    }
}

impl From<bool> for AllowPrivateNetwork {
    fn from(v: bool) -> Self {
        match v {
            true => Self(AllowPrivateNetworkInner::Yes),
            false => Self(AllowPrivateNetworkInner::No),
        }
    }
}

impl fmt::Debug for AllowPrivateNetwork {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            AllowPrivateNetworkInner::Yes => f.debug_tuple("Yes").finish(),
            AllowPrivateNetworkInner::No => f.debug_tuple("No").finish(),
            AllowPrivateNetworkInner::Dynamic(_) => f.debug_tuple("Predicate").finish(),
            AllowPrivateNetworkInner::DynamicAsync(_) => f.debug_tuple("AsyncPredicate").finish(),
        }
    }
}

#[derive(Clone)]
enum AllowPrivateNetworkInner {
    Yes,
    No,
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

impl Default for AllowPrivateNetworkInner {
    fn default() -> Self {
        Self::No
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::declare_interior_mutable_const,
        clippy::borrow_interior_mutable_const
    )]
    use salvo_core::http::{HeaderName, HeaderValue, Request, header::ORIGIN};
    use salvo_core::prelude::*;
    use salvo_core::test::TestClient;

    use super::AllowPrivateNetwork;
    use crate::Cors;

    const REQUEST_PRIVATE_NETWORK: HeaderName =
        HeaderName::from_static("access-control-request-private-network");
    const ALLOW_PRIVATE_NETWORK: HeaderName =
        HeaderName::from_static("access-control-allow-private-network");
    const TRUE: HeaderValue = HeaderValue::from_static("true");

    #[handler]
    async fn hello() -> &'static str {
        "hello"
    }

    #[tokio::test]
    async fn cors_private_network_header_is_added_correctly() {
        let cors_handler = Cors::new().allow_private_network(true).into_handler();

        let router = Router::new()
            .hoop(cors_handler)
            .push(Router::with_path("hello").goal(hello));
        let service = Service::new(router);

        let res = TestClient::options("https://google.com/hello")
            .add_header(REQUEST_PRIVATE_NETWORK, TRUE, true)
            .send(&service)
            .await;

        assert_eq!(res.headers().get(ALLOW_PRIVATE_NETWORK).unwrap(), TRUE);

        let res = TestClient::options("https://google.com/hello")
            .send(&service)
            .await;

        assert!(res.headers().get(ALLOW_PRIVATE_NETWORK).is_none());
    }

    #[tokio::test]
    async fn cors_private_network_header_is_added_correctly_with_predicate() {
        let allow_private_network =
            AllowPrivateNetwork::dynamic(|origin: Option<&HeaderValue>, req: &Request, _depot| {
                if req.uri().path() == "/allow-private"
                    && origin == Some(&HeaderValue::from_static("localhost"))
                {
                    Some(TRUE)
                } else {
                    None
                }
            });
        let cors_handler = Cors::new()
            .allow_private_network(allow_private_network)
            .into_handler();

        let router = Router::new().push(Router::with_path("{**}").goal(hello));
        let service = Service::new(router).hoop(cors_handler);

        let res = TestClient::options("https://localhost/allow-private")
            .add_header(ORIGIN, "localhost", true)
            .add_header(REQUEST_PRIVATE_NETWORK, TRUE, true)
            .send(&service)
            .await;

        assert_eq!(res.headers().get(ALLOW_PRIVATE_NETWORK).unwrap(), TRUE);

        let res = TestClient::options("https://localhost/other")
            .add_header(ORIGIN, "localhost", true)
            .add_header(REQUEST_PRIVATE_NETWORK, TRUE, true)
            .send(&service)
            .await;
        assert!(res.headers().get(ALLOW_PRIVATE_NETWORK).is_none());

        let res = TestClient::options("https://localhost/allow-private")
            .add_header(ORIGIN, "not-localhost", true)
            .add_header(REQUEST_PRIVATE_NETWORK, TRUE, true)
            .send(&service)
            .await;
        assert!(res.headers().get(ALLOW_PRIVATE_NETWORK).is_none());
    }
}
