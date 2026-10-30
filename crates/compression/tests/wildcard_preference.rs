#![cfg(all(feature = "gzip", feature = "deflate"))]
#![allow(missing_docs, clippy::unwrap_used)]

use salvo_compression::{Compression, CompressionLevel};
use salvo_core::http::header::CONTENT_ENCODING;
use salvo_core::prelude::*;
use salvo_core::test::TestClient;

#[handler]
async fn representation() -> String {
    "a".repeat(4096)
}

#[tokio::test]
async fn wildcard_participates_in_client_quality_order() {
    let compression = Compression::new()
        .disable_all()
        .enable_gzip(CompressionLevel::Default)
        .enable_deflate(CompressionLevel::Default);
    let service = Service::new(Router::new().hoop(compression).get(representation));
    for (accept, expected) in [
        ("gzip;q=0.1, *;q=1", "deflate"),
        ("*;q=1, gzip;q=0.1", "deflate"),
        ("gzip;q=0.9, *;q=0.5", "gzip"),
        ("gzip;q=0, *;q=1", "deflate"),
        ("gzip;q=0.1, deflate;q=0.2, *;q=1", "deflate"),
        ("gzip;q=0.9, deflate;q=0, *;q=1", "gzip"),
        ("gzip;q=0.5, *;q=0.5", "gzip"),
        ("deflate;q=0.5, *;q=0.5", "deflate"),
        ("gzip;q=0.1, *;q=0", "gzip"),
        ("unknown;q=1, *;q=0.5", "gzip"),
    ] {
        let response = TestClient::get("http://localhost/")
            .add_header("accept-encoding", accept, true)
            .send(&service)
            .await;
        assert_eq!(response.headers()[CONTENT_ENCODING], expected, "{accept}");
    }
}

#[tokio::test]
async fn forced_server_priority_still_ignores_relative_quality() {
    let compression = Compression::new()
        .disable_all()
        .enable_gzip(CompressionLevel::Default)
        .enable_deflate(CompressionLevel::Default)
        .force_priority(true);
    let service = Service::new(Router::new().hoop(compression).get(representation));
    let response = TestClient::get("http://localhost/")
        .add_header("accept-encoding", "gzip;q=0.1, *;q=1", true)
        .send(&service)
        .await;
    assert_eq!(response.headers()[CONTENT_ENCODING], "gzip");
}
