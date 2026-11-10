//! Explicit identity preference participates in default client-priority negotiation.
#![cfg(feature = "gzip")]

use salvo_compression::Compression;
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};

#[handler]
async fn content(req: &mut Request, res: &mut Response) {
    if req.uri().path() == "/not-modified" {
        res.status_code(StatusCode::NOT_MODIFIED);
    } else {
        res.render("identity preference ".repeat(128));
    }
}

#[tokio::test]
async fn client_priority_compares_explicit_identity_with_codecs_and_wildcards() {
    let service = Service::new(
        Router::with_path("{**path}")
            .hoop(Compression::new())
            .goal(content),
    );
    for (preference, encoded) in [
        ("identity;q=1, gzip;q=0.1", false),
        ("gzip;q=0.1, identity;q=1", false),
        ("identity;q=0.1, gzip;q=1", true),
        ("identity;q=0, gzip;q=1", true),
        ("identity;q=0.9, *;q=0.1", false),
        ("identity;q=0.1, *;q=1", true),
        ("identity;q=1, gzip;q=0", false),
        ("identity", false),
    ] {
        let mut response = TestClient::get("http://localhost/content")
            .add_header("accept-encoding", preference, true)
            .send(&service)
            .await;
        assert_eq!(
            response.headers().contains_key("content-encoding"),
            encoded,
            "{preference}"
        );
        assert_eq!(
            response.take_string().await.unwrap(),
            "identity preference ".repeat(128)
        );
        let response = TestClient::head("http://localhost/content")
            .add_header("accept-encoding", preference, true)
            .send(&service)
            .await;
        assert_eq!(
            response.headers().contains_key("content-encoding"),
            encoded,
            "HEAD {preference}"
        );
        let response = TestClient::get("http://localhost/not-modified")
            .add_header("accept-encoding", preference, true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NOT_MODIFIED));
        assert!(!response.headers().contains_key("content-encoding"));
    }
}

#[tokio::test]
async fn server_priority_can_still_choose_an_accepted_codec() {
    let service = Service::new(
        Router::with_path("{**path}")
            .hoop(Compression::new().force_priority(true))
            .goal(content),
    );
    let response = TestClient::get("http://localhost/content")
        .add_header("accept-encoding", "identity;q=1, gzip;q=0.1", true)
        .send(&service)
        .await;
    assert_eq!(response.headers()["content-encoding"], "gzip");
}
