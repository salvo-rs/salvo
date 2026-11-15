//! Client-IP forwarding rebuilds the forwarding header from the direct peer.
use std::convert::Infallible;

use hyper::upgrade::OnUpgrade;
use salvo_core::conn::SocketAddr;
use salvo_core::http::{ReqBody, ResBody, StatusCode};
use salvo_core::test::TestClient;
use salvo_core::{Response, Router, Service};
use salvo_proxy::{Client, Proxy};

struct CaptureClient;

impl Client for CaptureClient {
    type Error = Infallible;

    async fn execute(
        &self,
        request: hyper::Request<ReqBody>,
        _upgrade: Option<OnUpgrade>,
    ) -> Result<hyper::Response<ResBody>, Self::Error> {
        let mut response = hyper::Response::new(ResBody::None);
        for value in request.headers().get_all("x-forwarded-for") {
            response
                .headers_mut()
                .append("x-upstream-xff", value.clone());
        }
        Ok(response)
    }
}

async fn forwarded_response(remote: SocketAddr, forwarding: bool) -> Response {
    let proxy =
        Proxy::new("http://upstream.invalid", CaptureClient).client_ip_forwarding(forwarding);
    let service = Service::new(Router::new().goal(proxy));
    let mut request = TestClient::get("http://gateway/")
        .add_header("x-forwarded-for", "127.0.0.1", true)
        .add_header("x-forwarded-for", "10.0.0.1, 127.0.0.1", false)
        .build();
    *request.remote_addr_mut() = remote;
    let response = service.handle(request).await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    response
}

#[tokio::test]
async fn unknown_peer_drops_all_inbound_forwarded_for_values() {
    let response = forwarded_response(SocketAddr::Unknown, true).await;
    assert!(!response.headers().contains_key("x-upstream-xff"));
}

#[tokio::test]
async fn ip_peer_replaces_the_entire_untrusted_chain() {
    for address in ["192.0.2.7:12345", "[2001:db8::7]:12345"] {
        let address = address.parse::<std::net::SocketAddr>().unwrap();
        let response = forwarded_response(address.into(), true).await;
        let values = response.headers().get_all("x-upstream-xff");
        assert_eq!(values.iter().count(), 1);
        assert_eq!(
            values.iter().next().unwrap().to_str().unwrap(),
            address.ip().to_string()
        );
    }
}

#[tokio::test]
async fn disabled_forwarding_preserves_both_original_header_values() {
    let response = forwarded_response(SocketAddr::Unknown, false).await;
    let values = response.headers().get_all("x-upstream-xff");
    assert_eq!(
        values
            .iter()
            .map(|value| value.to_str().unwrap())
            .collect::<Vec<_>>(),
        ["127.0.0.1", "10.0.0.1, 127.0.0.1"]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn unix_peer_drops_all_inbound_forwarded_for_values() {
    let (stream, _other) = tokio::net::UnixStream::pair().unwrap();
    let response = forwarded_response(stream.peer_addr().unwrap().into(), true).await;
    assert!(!response.headers().contains_key("x-upstream-xff"));
}
