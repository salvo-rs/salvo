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
