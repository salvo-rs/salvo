#![cfg(feature = "embed")]
#![allow(missing_docs, clippy::unwrap_used)]

use rust_embed::RustEmbed;
use salvo_core::http::Method;
use salvo_core::prelude::*;
use salvo_core::test::{RequestBuilder, ResponseExt, TestClient};
use salvo_serve_static::{EmbeddedFileExt, StaticEmbed};

#[derive(RustEmbed)]
#[folder = "test/static/"]
struct Assets;

#[tokio::test]
async fn repeated_if_none_match_fields_share_combined_list_semantics() {
    for service in [
        Service::new(Router::with_path("{**path}").goal(StaticEmbed::<Assets>::new())),
        Service::new(
            Router::with_path("{**path}").goal(Assets::get("test1.txt").unwrap().into_handler()),
        ),
    ] {
        let original = TestClient::get("http://localhost/test1.txt")
            .send(&service)
            .await;
        let etag = original.headers()["etag"].to_str().unwrap();
        for method in [Method::GET, Method::HEAD] {
            for (values, expected) in [
                (
                    vec!["\"stale\"".to_owned(), etag.to_owned()],
                    StatusCode::NOT_MODIFIED,
                ),
                (
                    vec![etag.to_owned(), "\"stale\"".to_owned()],
                    StatusCode::NOT_MODIFIED,
                ),
                (
                    vec!["\"stale\"".to_owned(), format!("W/{etag}")],
                    StatusCode::NOT_MODIFIED,
                ),
                (
                    vec![
                        "\"stale,one\"".to_owned(),
                        format!("\"stale,two\", W/{etag}"),
                    ],
                    StatusCode::NOT_MODIFIED,
                ),
                (
                    vec!["\"stale\"".to_owned(), "\"other\"".to_owned()],
                    StatusCode::OK,
                ),
            ] {
                let mut combined =
                    RequestBuilder::new("http://localhost/test1.txt", method.clone())
                        .add_header("if-none-match", values.join(", "), true)
                        .send(&service)
                        .await;
                let mut split = RequestBuilder::new("http://localhost/test1.txt", method.clone());
                for value in values {
                    split = split.add_header("if-none-match", value, false);
                }
                let mut split = split.send(&service).await;
                assert_eq!(combined.status_code, Some(expected));
                assert_eq!(split.status_code, combined.status_code);
                assert_eq!(split.headers()["etag"], combined.headers()["etag"]);
                assert_eq!(
                    split.take_bytes(None).await.unwrap(),
                    combined.take_bytes(None).await.unwrap()
                );
            }
        }
        let rejected = TestClient::get("http://localhost/test1.txt")
            .add_header("if-match", "\"different\"", true)
            .add_header("if-none-match", "\"stale\"", false)
            .add_header("if-none-match", etag, false)
            .send(&service)
            .await;
        assert_eq!(rejected.status_code, Some(StatusCode::PRECONDITION_FAILED));
    }
}
