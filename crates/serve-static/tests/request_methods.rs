#![allow(missing_docs, clippy::unwrap_used)]

use salvo_core::fs::NamedFile;
use salvo_core::http::Method;
use salvo_core::prelude::*;
use salvo_core::test::{RequestBuilder, ResponseExt, TestClient};
use salvo_serve_static::{StaticDir, StaticFile};

#[handler]
async fn named_file() -> NamedFile {
    NamedFile::open("test/static/test1.txt").await.unwrap()
}

fn services() -> Vec<Service> {
    let services = vec![
        Service::new(Router::with_path("{**path}").goal(named_file)),
        Service::new(Router::with_path("{**path}").goal(StaticFile::new("test/static/test1.txt"))),
        Service::new(Router::with_path("{**path}").goal(StaticDir::new("test/static"))),
    ];
    #[cfg(feature = "embed")]
    let services = {
        use rust_embed::RustEmbed;
        #[derive(RustEmbed)]
        #[folder = "test/static/"]
        struct Assets;
        let mut services = services;
        services.push(Service::new(
            Router::with_path("{**path}").goal(salvo_serve_static::StaticEmbed::<Assets>::new()),
        ));
        services
    };
    services
}

#[tokio::test]
async fn ranges_only_apply_to_get_for_every_file_handler() {
    for service in services() {
        for method in [
            Method::GET,
            Method::HEAD,
            Method::POST,
            Method::PUT,
            Method::from_bytes(b"REPORT").unwrap(),
        ] {
            for range in ["bytes=0-1", "bytes=999999-", "bytes=invalid"] {
                let mut response =
                    RequestBuilder::new("http://localhost/test1.txt", method.clone())
                        .add_header("range", range, true)
                        .send(&service)
                        .await;
                if method == Method::GET {
                    assert_eq!(
                        response.status_code,
                        Some(if range == "bytes=0-1" {
                            StatusCode::PARTIAL_CONTENT
                        } else {
                            StatusCode::RANGE_NOT_SATISFIABLE
                        })
                    );
                } else {
                    assert_eq!(
                        response.status_code,
                        Some(StatusCode::OK),
                        "{method} {range}"
                    );
                    assert!(!response.headers().contains_key("content-range"));
                    let body = response.take_bytes(None).await.unwrap();
                    if method == Method::HEAD {
                        assert!(body.is_empty());
                    } else {
                        assert_eq!(body.as_ref(), include_bytes!("../test/static/test1.txt"));
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn conditional_status_uses_actual_method_for_every_file_handler() {
    for service in services() {
        let original = TestClient::get("http://localhost/test1.txt")
            .send(&service)
            .await;
        let etag = original.headers()["etag"].to_str().unwrap();
        for method in [Method::GET, Method::HEAD, Method::POST, Method::PUT] {
            let retrieval = method == Method::GET || method == Method::HEAD;
            for validator in [etag.to_owned(), format!("W/{etag}"), "*".to_owned()] {
                let response = RequestBuilder::new("http://localhost/test1.txt", method.clone())
                    .add_header("if-none-match", validator, true)
                    .add_header("range", "bytes=999999-", true)
                    .send(&service)
                    .await;
                assert_eq!(
                    response.status_code,
                    Some(if retrieval {
                        StatusCode::NOT_MODIFIED
                    } else {
                        StatusCode::PRECONDITION_FAILED
                    }),
                    "{method}"
                );
                assert!(!response.headers().contains_key("content-range"));
            }
            let response = RequestBuilder::new("http://localhost/test1.txt", method.clone())
                .add_header("if-none-match", "\"different\"", true)
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::OK));
            let response = RequestBuilder::new("http://localhost/test1.txt", method)
                .add_header("if-match", "\"different\"", true)
                .add_header("if-none-match", etag, true)
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::PRECONDITION_FAILED));
        }
    }
}

#[tokio::test]
async fn modified_since_only_applies_to_retrieval_methods() {
    for service in services() {
        let original = TestClient::get("http://localhost/test1.txt")
            .send(&service)
            .await;
        let Some(last_modified) = original.headers().get("last-modified") else {
            continue;
        };
        for method in [Method::GET, Method::HEAD, Method::POST, Method::PUT] {
            let retrieval = method == Method::GET || method == Method::HEAD;
            let response = RequestBuilder::new("http://localhost/test1.txt", method)
                .add_header("if-modified-since", last_modified.clone(), true)
                .send(&service)
                .await;
            assert_eq!(
                response.status_code,
                Some(if retrieval {
                    StatusCode::NOT_MODIFIED
                } else {
                    StatusCode::OK
                })
            );
        }
    }
}

#[tokio::test]
async fn failed_non_retrieval_conditions_do_not_label_error_pages_as_encoded() {
    let services = vec![Service::new(
        Router::with_path("{**path}").goal(StaticFile::new("test/static/logo.svgz")),
    )];
    #[cfg(feature = "embed")]
    let services = {
        use rust_embed::RustEmbed;
        #[derive(RustEmbed)]
        #[folder = "test/static/"]
        struct Assets;
        let mut services = services;
        services.push(Service::new(
            Router::with_path("{**path}").goal(salvo_serve_static::StaticEmbed::<Assets>::new()),
        ));
        services
    };
    for service in services {
        let original = TestClient::get("http://localhost/logo.svgz")
            .send(&service)
            .await;
        let etag = original.headers()["etag"].to_str().unwrap();
        assert_eq!(original.headers()["content-encoding"], "gzip");
        let response = TestClient::post("http://localhost/logo.svgz")
            .add_header("if-none-match", etag, true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::PRECONDITION_FAILED));
        assert!(!response.headers().contains_key("content-encoding"));
    }
}
