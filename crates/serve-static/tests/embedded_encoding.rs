#![cfg(feature = "embed")]

use rust_embed::RustEmbed;
use salvo_core::http::{Method, StatusCode};
use salvo_core::test::{RequestBuilder, ResponseExt, TestClient};
use salvo_core::{Response, Router, Service};
use salvo_serve_static::{EmbeddedFileExt, StaticEmbed, render_embedded_file};

#[derive(RustEmbed)]
#[folder = "test/static/"]
struct Assets;

#[tokio::test]
async fn embedded_self_coded_files_advertise_their_stored_encoding() {
    let service = Service::new(
        Router::with_path("{**path}").goal(
            StaticEmbed::<Assets>::new()
                .defaults("logo.svgz")
                .fallback("logo.svgz"),
        ),
    );
    let expected = Assets::get("logo.svgz").unwrap().data;
    for path in ["/logo.svgz", "/", "/client/route.css"] {
        for method in [Method::GET, Method::HEAD] {
            let mut response =
                RequestBuilder::new(format!("http://localhost{path}"), method.clone())
                    .send(&service)
                    .await;
            assert_eq!(response.status_code, Some(StatusCode::OK));
            assert_eq!(response.headers()["content-type"], "image/svg+xml");
            assert_eq!(
                response
                    .headers()
                    .get("content-encoding")
                    .and_then(|v| v.to_str().ok()),
                Some("gzip"),
                "{method} {path} omitted the embedded file coding"
            );
            assert_eq!(
                response.headers()["content-length"],
                expected.len().to_string()
            );
            let body = response.take_bytes(None).await.unwrap();
            if method == Method::HEAD {
                assert!(body.is_empty());
            } else {
                assert_eq!(body.as_ref(), expected.as_ref());
            }
        }
    }
}

#[tokio::test]
async fn direct_embedded_handlers_preserve_explicit_codings_and_gzip_downloads() {
    let service = Service::new(
        Router::new()
            .push(
                Router::with_path("logo.SVGZ")
                    .goal(Assets::get("logo.svgz").unwrap().into_handler()),
            )
            .push(
                Router::with_path("archive.gz")
                    .goal(Assets::get("test1.txt.gz").unwrap().into_handler()),
            ),
    );
    let response = TestClient::get("http://localhost/logo.SVGZ")
        .send(&service)
        .await;
    assert_eq!(response.headers()["content-encoding"], "gzip");
    let response = TestClient::get("http://localhost/archive.gz")
        .send(&service)
        .await;
    assert!(!response.headers().contains_key("content-encoding"));

    let request = TestClient::get("http://localhost/logo.svgz").build();
    let mut response = Response::new();
    response
        .headers_mut()
        .insert("content-encoding", "custom".parse().unwrap());
    render_embedded_file(
        Assets::get("logo.svgz").unwrap(),
        &request,
        &mut response,
        None,
    );
    assert_eq!(response.headers()["content-encoding"], "custom");

    let request = TestClient::get("http://localhost/scene.x3dvz").build();
    let mut response = Response::new();
    render_embedded_file(
        Assets::get("logo.svgz").unwrap(),
        &request,
        &mut response,
        Some("text/plain".parse().unwrap()),
    );
    assert_eq!(response.headers()["content-encoding"], "gzip");
    assert_eq!(response.headers()["content-type"], "text/plain");
}
