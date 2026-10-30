#![cfg(all(feature = "fixed-guard", feature = "moka-store"))]

use std::net::{IpAddr, SocketAddr};

use salvo_core::prelude::*;
use salvo_core::test::TestClient;
use salvo_rate_limiter::{
    BasicQuota, FixedGuard, MokaStore, RateIssuer, RateLimiter, RemoteIpIssuer, TrustedProxyIssuer,
};

fn request(fields: &[&str]) -> Request {
    let mut req = TestClient::get("http://example.test/").build();
    *req.remote_addr_mut() = "10.0.0.6:443".parse::<SocketAddr>().unwrap().into();
    for field in fields {
        req.headers_mut()
            .append("x-forwarded-for", field.parse().unwrap());
    }
    req
}

fn issuer() -> TrustedProxyIssuer {
    TrustedProxyIssuer::new([
        "10.0.0.5".parse::<IpAddr>().unwrap(),
        "10.0.0.6".parse::<IpAddr>().unwrap(),
    ])
}

#[tokio::test]
async fn repeated_forwarded_fields_are_one_ordered_chain() {
    for (fields, expected) in [
        (vec!["192.0.2.1", "198.51.100.7"], "198.51.100.7"),
        (vec!["192.0.2.1", "198.51.100.7, 10.0.0.5"], "198.51.100.7"),
        (vec!["192.0.2.1", "2001:db8::7", "10.0.0.5"], "2001:db8::7"),
        (vec!["unknown", "198.51.100.7"], "198.51.100.7"),
        (vec!["unknown, 198.51.100.7", "10.0.0.5"], "198.51.100.7"),
    ] {
        let expected = expected.parse::<IpAddr>().unwrap();
        let combined = fields.join(", ");
        assert_eq!(
            issuer()
                .issue(&mut request(&[&combined]), &Depot::new())
                .await,
            Some(expected)
        );
        assert_eq!(
            issuer().issue(&mut request(&fields), &Depot::new()).await,
            Some(expected)
        );
    }
}

#[tokio::test]
async fn direct_clients_and_missing_forwarded_fields_keep_their_identity() {
    let peer = "10.0.0.6".parse::<IpAddr>().unwrap();
    let mut req = request(&["192.0.2.1", "198.51.100.7"]);
    assert_eq!(
        RemoteIpIssuer.issue(&mut req, &Depot::new()).await,
        Some(peer)
    );
    *req.remote_addr_mut() = "203.0.113.5:443".parse::<SocketAddr>().unwrap().into();
    assert_eq!(
        issuer().issue(&mut req, &Depot::new()).await,
        Some("203.0.113.5".parse().unwrap())
    );
    assert_eq!(
        issuer().issue(&mut request(&[]), &Depot::new()).await,
        Some(peer)
    );
    let mut req = request(&[]);
    req.headers_mut()
        .insert("x-real-ip", "198.51.100.7".parse().unwrap());
    assert_eq!(
        issuer().issue(&mut req, &Depot::new()).await,
        Some("198.51.100.7".parse().unwrap())
    );
}

#[tokio::test]
async fn malformed_hops_do_not_expose_untrusted_left_hand_addresses() {
    for fields in [
        vec!["192.0.2.1, unknown, 10.0.0.5"],
        vec!["192.0.2.1", "unknown, 10.0.0.5"],
        vec!["192.0.2.1", ""],
    ] {
        let mut req = request(&fields);
        req.headers_mut()
            .insert("x-real-ip", "192.0.2.9".parse().unwrap());
        assert_eq!(
            issuer().issue(&mut req, &Depot::new()).await,
            Some("10.0.0.6".parse().unwrap())
        );
    }
    let mut req = request(&["192.0.2.1"]);
    req.headers_mut().append(
        "x-forwarded-for",
        salvo_core::http::HeaderValue::from_bytes(b"\x80").unwrap(),
    );
    assert_eq!(
        issuer().issue(&mut req, &Depot::new()).await,
        Some("10.0.0.6".parse().unwrap())
    );
}

#[tokio::test]
async fn spoofed_first_field_cannot_create_fresh_rate_limit_keys() {
    #[handler]
    async fn endpoint() -> &'static str {
        "ok"
    }

    let service = Service::new(
        Router::new()
            .hoop(RateLimiter::new(
                FixedGuard::new(),
                MokaStore::<IpAddr, FixedGuard>::default(),
                issuer(),
                BasicQuota::per_hour(1),
            ))
            .get(endpoint),
    );
    for (spoof, expected) in [
        ("192.0.2.1", StatusCode::OK),
        ("192.0.2.2", StatusCode::TOO_MANY_REQUESTS),
        ("192.0.2.3", StatusCode::TOO_MANY_REQUESTS),
    ] {
        let response = service
            .handle(request(&[spoof, "198.51.100.7, 10.0.0.5"]))
            .await;
        assert_eq!(response.status_code, Some(expected));
    }
}
