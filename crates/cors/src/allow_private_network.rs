use std::sync::Arc;

use salvo_core::http::header::{HeaderName, HeaderValue};
use salvo_core::{Depot, Request};

use super::inner::BoolInner;

/// Holds configuration for how to set the [`Access-Control-Allow-Private-Network`][wicg] header.
///
/// See [`Cors::allow_private_network`] for more details.
///
/// [wicg]: https://wicg.github.io/private-network-access/
/// [`Cors::allow_private_network`]: super::Cors::allow_private_network
#[derive(Clone, Default, Debug)]
#[must_use]
pub struct AllowPrivateNetwork(BoolInner);

impl AllowPrivateNetwork {
    /// Allow requests via a more private network than the one used to access the origin
    ///
    /// See [`Cors::allow_private_network`] for more details.
    ///
    /// [`Cors::allow_private_network`]: super::Cors::allow_private_network
    pub fn yes() -> Self {
        Self(BoolInner::Yes)
    }

    /// Allow requests via private network for some requests by a closure
    ///
    /// See [`Cors::allow_private_network`] for more details.
    ///
    /// [`Cors::allow_private_network`]: super::Cors::allow_private_network
    pub fn dynamic<C>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> bool + Send + Sync + 'static,
    {
        Self(BoolInner::Dynamic(Arc::new(c)))
    }

    /// Allow private-network requests for some requests by an async closure.
    ///
    /// See [`Cors::allow_private_network`] for more details.
    ///
    /// [`Cors::allow_private_network`]: super::Cors::allow_private_network
    pub fn dynamic_async<C, Fut>(c: C) -> Self
    where
        C: Fn(Option<&HeaderValue>, &Request, &Depot) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = bool> + Send + 'static,
    {
        Self(BoolInner::DynamicAsync(Arc::new(
            move |header, req, depot| Box::pin(c(header, req, depot)),
        )))
    }

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

        if req.headers().get(REQUEST_PRIVATE_NETWORK) != Some(&TRUE) {
            return None;
        }

        self.0
            .resolve_async(origin, req, depot)
            .await
            .then_some((ALLOW_PRIVATE_NETWORK, TRUE))
    }
}

impl From<bool> for AllowPrivateNetwork {
    fn from(v: bool) -> Self {
        match v {
            true => Self(BoolInner::Yes),
            false => Self(BoolInner::No),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::declare_interior_mutable_const,
        clippy::borrow_interior_mutable_const
    )]
    use salvo_core::http::header::ORIGIN;
    use salvo_core::http::{HeaderName, HeaderValue, Request};
    use salvo_core::prelude::*;
    use salvo_core::test::TestClient;

    use super::super::inner::BoolInner;
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

    #[test]
    fn test_from_bool() {
        let p: AllowPrivateNetwork = true.into();
        assert!(matches!(p.0, BoolInner::Yes));

        let p: AllowPrivateNetwork = false.into();
        assert!(matches!(p.0, BoolInner::No));
    }

    #[tokio::test]
    async fn test_to_header() {
        let mut req = Request::default();
        let depot = Depot::new();
        let origin = HeaderValue::from_static("https://example.com");

        // Test `Yes` without request header
        let p = AllowPrivateNetwork::yes();
        let header = p.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(header, None);

        // Test `Yes` with request header
        req.headers_mut()
            .insert(REQUEST_PRIVATE_NETWORK, HeaderValue::from_static("true"));
        let header = p.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((ALLOW_PRIVATE_NETWORK, HeaderValue::from_static("true")))
        );

        // Test `No`
        let p: AllowPrivateNetwork = false.into();
        let header = p.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(header, None);

        // Test `Dynamic`
        let p = AllowPrivateNetwork::dynamic(|_, _, _| true);
        let header = p.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((ALLOW_PRIVATE_NETWORK, HeaderValue::from_static("true")))
        );

        // Test `DynamicAsync`
        let p = AllowPrivateNetwork::dynamic_async(|_, _, _| async { true });
        let header = p.to_header(Some(&origin), &req, &depot).await;
        assert_eq!(
            header,
            Some((ALLOW_PRIVATE_NETWORK, HeaderValue::from_static("true")))
        );
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
                req.uri().path() == "/allow-private"
                    && origin == Some(&HeaderValue::from_static("localhost"))
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
