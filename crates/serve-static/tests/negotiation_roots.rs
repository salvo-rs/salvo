//! Static representation selection and root priority share one contract.
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_serve_static::StaticDir;

#[tokio::test]
async fn wildcard_selects_an_acceptable_sidecar_or_reports_no_representation() {
    let service = Service::new(Router::with_path("{**path}").goal(StaticDir::new("test/static")));
    let mut compressed = TestClient::get("http://localhost/test1.txt")
        .add_header("accept-encoding", "*;q=1, identity;q=0", true)
        .send(&service)
        .await;
    assert_eq!(compressed.status_code, Some(StatusCode::OK));
    assert!(compressed.headers().contains_key("content-encoding"));
    assert!(!compressed.take_bytes(None).await.unwrap().is_empty());
    for encoding in [
        "*;q=0, identity;q=0",
        "gzip;q=0, br;q=0, *;q=1, identity;q=0",
    ] {
        let response = TestClient::get("http://localhost/test1.txt")
            .add_header("accept-encoding", encoding, true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NOT_ACCEPTABLE));
    }
}

#[tokio::test]
async fn file_and_directory_lookup_both_use_the_first_matching_root() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first");
    let last = directory.path().join("last");
    for root in [&first, &last] {
        std::fs::create_dir_all(root.join("folder")).unwrap();
    }
    std::fs::write(first.join("asset.txt"), "first").unwrap();
    std::fs::write(last.join("asset.txt"), "last").unwrap();
    std::fs::write(first.join("folder/index.txt"), "first").unwrap();
    std::fs::write(last.join("folder/index.txt"), "last").unwrap();
    let service = Service::new(
        Router::with_path("{**path}").goal(StaticDir::new(vec![first, last]).defaults("index.txt")),
    );
    let mut direct = TestClient::get("http://localhost/asset.txt")
        .send(&service)
        .await;
    let mut default = TestClient::get("http://localhost/folder/")
        .send(&service)
        .await;
    assert_eq!(direct.take_string().await.unwrap(), "first");
    assert_eq!(default.take_string().await.unwrap(), "first");
}

#[tokio::test]
async fn stored_gzip_resources_honor_acceptability_and_vary() {
    let service = Service::new(Router::with_path("{**path}").goal(StaticDir::new("test/static")));
    for (accept, expected) in [
        (None, StatusCode::OK),
        (Some("gzip"), StatusCode::OK),
        (Some("x-gzip"), StatusCode::OK),
        (Some("*;q=1, identity;q=0"), StatusCode::OK),
        (Some("*;q=0, gzip;q=1"), StatusCode::OK),
        (Some("gzip;q=0, *;q=1"), StatusCode::NOT_ACCEPTABLE),
        (Some("x-gzip;q=0, *;q=1"), StatusCode::NOT_ACCEPTABLE),
        (Some("*;q=0"), StatusCode::NOT_ACCEPTABLE),
        (Some("identity"), StatusCode::NOT_ACCEPTABLE),
        (Some("br"), StatusCode::NOT_ACCEPTABLE),
        (Some(""), StatusCode::NOT_ACCEPTABLE),
    ] {
        let mut request = TestClient::get("http://localhost/logo.svgz");
        if let Some(accept) = accept {
            request = request.add_header("accept-encoding", accept, true);
        }
        let response = request.send(&service).await;
        assert_eq!(
            response.status_code,
            Some(expected),
            "Accept-Encoding: {accept:?}"
        );
        assert_eq!(response.headers()["vary"], "accept-encoding");
        if expected == StatusCode::OK {
            assert_eq!(response.headers()["content-encoding"], "gzip");
        } else {
            assert!(!response.headers().contains_key("content-encoding"));
        }
    }
}

#[tokio::test]
async fn root_priority_precedes_negotiation_and_is_preserved_for_fallback() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing");
    let first = directory.path().join("first");
    let last = directory.path().join("last");
    for root in [&first, &last] {
        std::fs::create_dir(root).unwrap();
    }
    std::fs::write(first.join("asset.txt"), "first").unwrap();
    std::fs::write(last.join("asset.txt"), "last").unwrap();
    std::fs::write(
        last.join("asset.txt.gz"),
        include_bytes!("../test/static/test1.txt.gz"),
    )
    .unwrap();
    std::fs::write(first.join("fallback.txt"), "first fallback").unwrap();
    std::fs::write(last.join("fallback.txt"), "last fallback").unwrap();
    let service = Service::new(
        Router::with_path("{**path}")
            .goal(StaticDir::new(vec![missing, first, last]).fallback("fallback.txt")),
    );
    let response = TestClient::get("http://localhost/asset.txt")
        .add_header("accept-encoding", "gzip, identity;q=0", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::NOT_ACCEPTABLE));
    for path in ["missing.txt", ".hidden"] {
        let mut response = TestClient::get(format!("http://localhost/{path}"))
            .send(&service)
            .await;
        assert_eq!(response.take_string().await.unwrap(), "first fallback");
    }
}

#[tokio::test]
async fn legacy_gzip_alias_selects_a_sidecar_and_overrides_wildcards() {
    let service = Service::new(Router::with_path("{**path}").goal(StaticDir::new("test/static")));
    let response = TestClient::get("http://localhost/test1.txt")
        .add_header("accept-encoding", "x-gzip, identity;q=0", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(response.headers()["content-encoding"], "gzip");
    let response = TestClient::get("http://localhost/test1.txt")
        .add_header(
            "accept-encoding",
            "x-gzip;q=0, br;q=0, *;q=1, identity;q=0",
            true,
        )
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::NOT_ACCEPTABLE));
}
