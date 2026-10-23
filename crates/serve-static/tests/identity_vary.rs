#![allow(missing_docs, clippy::unwrap_used)]

use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_serve_static::StaticDir;

#[tokio::test]
async fn static_identity_and_compressed_variants_share_vary_policy() {
    let service = Service::new(Router::with_path("{**path}").goal(StaticDir::new("test/static")));
    for encodings in [
        [None, Some("identity"), Some("gzip")],
        [Some("gzip"), Some("identity"), None],
    ] {
        for head in [false, true] {
            for encoding in encodings {
                let mut request = if head {
                    TestClient::head("http://localhost/test1.txt")
                } else {
                    TestClient::get("http://localhost/test1.txt")
                };
                if let Some(encoding) = encoding {
                    request = request.add_header("accept-encoding", encoding, true);
                }
                let mut response = request.send(&service).await;
                assert_eq!(response.status_code, Some(StatusCode::OK));
                assert_eq!(response.headers()["vary"], "accept-encoding");
                assert_eq!(
                    response.headers().contains_key("content-encoding"),
                    encoding == Some("gzip")
                );
                if head {
                    assert!(response.take_bytes(None).await.unwrap().is_empty());
                } else {
                    // The repository's sidecar is a selection fixture. Compare
                    // its wire bytes rather than asking the client to decode it.
                    let expected: &[u8] = if encoding == Some("gzip") {
                        include_bytes!("../test/static/test1.txt.gz")
                    } else {
                        include_bytes!("../test/static/test1.txt")
                    };
                    assert_eq!(response.take_bytes(None).await.unwrap().as_ref(), expected);
                }
            }
        }
    }
}

#[tokio::test]
async fn disabled_variants_do_not_add_vary() {
    let mut directory = StaticDir::new("test/static");
    directory.compressed_variations.clear();
    let service = Service::new(Router::with_path("{**path}").goal(directory));
    for encoding in ["identity", "gzip"] {
        let response = TestClient::get("http://localhost/test1.txt")
            .add_header("accept-encoding", encoding, true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert!(!response.headers().contains_key("vary"));
        assert!(!response.headers().contains_key("content-encoding"));
    }
}
