//! Encoding negotiation and integrity metadata regressions.
#![cfg(feature = "gzip")]
#![allow(missing_docs, clippy::unwrap_used)]

use salvo_compression::Compression;
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};

#[handler]
async fn hello(res: &mut Response) {
    res.add_header("trailer", "Content-Digest, x-complete, Repr-Digest", true)
        .unwrap();
    res.add_header(
        "content-digest",
        "sha-256=:LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ=:",
        true,
    )
    .unwrap();
    res.add_header(
        "repr-digest",
        "sha-256=:LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ=:",
        true,
    )
    .unwrap();
    res.render("hello");
}

#[tokio::test]
async fn identity_rejection_overrides_the_size_threshold() {
    let mut res = TestClient::get("http://localhost/")
        .add_header("accept-encoding", "gzip, identity;q=0", true)
        .send(Router::new().hoop(Compression::new()).get(hello))
        .await;
    assert_eq!(res.status_code, Some(StatusCode::OK));
    assert_eq!(res.headers()["content-encoding"], "gzip");
    assert_eq!(res.take_string().await.unwrap(), "hello");
}

#[tokio::test]
async fn identity_responses_vary_when_a_rejected_identity_would_change_the_result() {
    let service = Service::new(Router::new().hoop(Compression::new()).get(hello));
    for no_transform in [false, true] {
        let mut request =
            TestClient::get("http://localhost/").add_header("accept-encoding", "gzip", true);
        if no_transform {
            request = request.add_header("cache-control", "no-transform", true);
        }
        let mut response = request.send(&service).await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert!(!response.headers().contains_key("content-encoding"));
        assert!(response.headers().get_all("vary").iter().any(|value| {
            value
                .to_str()
                .unwrap()
                .split(',')
                .any(|name| name.trim().eq_ignore_ascii_case("accept-encoding"))
        }));
        assert_eq!(response.take_string().await.unwrap(), "hello");
    }
}

#[tokio::test]
async fn compression_removes_digests_for_different_wire_bytes() {
    let mut res = TestClient::get("http://localhost/")
        .add_header("accept-encoding", "gzip", true)
        .send(
            Router::new()
                .hoop(Compression::new().min_length(0))
                .get(hello),
        )
        .await;
    assert_eq!(res.headers()["content-encoding"], "gzip");
    assert!(!res.headers().contains_key("content-digest"));
    assert!(!res.headers().contains_key("repr-digest"));
    assert_eq!(res.headers()["trailer"], "x-complete");
    // ResponseExt's take_string decodes gzip; inspect the raw body instead.
    let raw = http_body_util::BodyExt::collect(res.take_body())
        .await
        .unwrap()
        .to_bytes();
    assert!(raw.starts_with(&[0x1f, 0x8b]));
    assert_ne!(&raw[..], b"hello");
}

#[tokio::test]
async fn no_acceptable_coding_returns_406() {
    let res = TestClient::get("http://localhost/")
        .add_header("accept-encoding", "*;q=0, identity;q=0", true)
        .send(Router::new().hoop(Compression::new()).get(hello))
        .await;
    assert_eq!(res.status_code, Some(StatusCode::NOT_ACCEPTABLE));
}

#[handler]
async fn bodyless(req: &mut Request, res: &mut Response) {
    res.status_code(StatusCode::from_u16(req.query::<u16>("status").unwrap()).unwrap());
}

#[tokio::test]
async fn bodyless_statuses_do_not_negotiate_a_prohibited_identity_body() {
    let service = Service::new(Router::new().hoop(Compression::new()).goal(bodyless));
    for status in [100, 101, 103, 204, 205, 304] {
        let response = TestClient::get(format!("http://localhost/?status={status}"))
            .add_header("accept-encoding", "identity;q=0, *;q=0", true)
            .send(&service)
            .await;
        assert_eq!(
            response.status_code,
            Some(StatusCode::from_u16(status).unwrap())
        );
        assert!(response.body.is_none());
        assert!(!response.headers().contains_key("content-encoding"));
    }
}
