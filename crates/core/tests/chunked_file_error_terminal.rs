//! A failed file read must leave the response body safe to poll to completion.

use http_body_util::BodyExt;
use salvo_core::fs::NamedFile;
use salvo_core::http::{HeaderMap, Response};

#[tokio::test]
async fn truncated_file_yields_one_error_then_eof() {
    let file = tempfile::NamedTempFile::new().unwrap();
    file.as_file().set_len(1024).unwrap();
    let named = NamedFile::builder(file.path())
        .preload_threshold(0)
        .build()
        .await
        .unwrap();
    let mut response = Response::new();
    named.send(&HeaderMap::new(), &mut response).await;
    file.as_file().set_len(0).unwrap();

    let mut body = response.take_body();
    assert!(body.frame().await.unwrap().is_err());
    assert!(body.frame().await.is_none());
}
