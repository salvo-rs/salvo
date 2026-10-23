#![cfg(feature = "gzip")]
#![allow(missing_docs, clippy::unwrap_used)]

use salvo_compression::Compression;
use salvo_core::http::Method;
use salvo_core::http::header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, VARY};
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};

#[handler]
async fn representation(req: &mut Request, res: &mut Response) {
    res.add_header(VARY, "origin", true).unwrap();
    res.add_header(CONTENT_TYPE, "text/plain", true).unwrap();
    if req.method() == Method::HEAD {
        res.add_header(CONTENT_LENGTH, "5", true).unwrap();
    } else if req.uri().path() == "/chunks" {
        res.write_body("he").unwrap();
        res.write_body("llo").unwrap();
    } else if req.uri().path() == "/stream" {
        res.stream(futures_util::stream::once(async {
            Ok::<_, std::io::Error>("hello")
        }));
    } else {
        res.write_body("hello").unwrap();
    }
}

#[tokio::test]
async fn every_negotiated_representation_varies_on_encoding() {
    let service = Service::new(
        Router::with_hoop(Compression::new().min_length(1))
            .push(Router::with_path("{kind}").goal(representation)),
    );
    for encodings in [
        [
            None,
            Some("identity"),
            Some("gzip"),
            Some("unknown"),
            Some("gzip;q=0"),
        ],
        [
            Some("gzip"),
            Some("gzip;q=0"),
            Some("unknown"),
            Some("identity"),
            None,
        ],
    ] {
        for path in ["once", "chunks", "stream"] {
            for head in [false, true] {
                for encoding in encodings {
                    let url = format!("http://localhost/{path}");
                    let mut request = if head {
                        TestClient::head(url)
                    } else {
                        TestClient::get(url)
                    };
                    if let Some(encoding) = encoding {
                        request = request.add_header("accept-encoding", encoding, true);
                    }
                    let mut response = request.send(&service).await;
                    let vary = response
                        .headers()
                        .get_all(VARY)
                        .iter()
                        .flat_map(|value| value.to_str().unwrap().split(','))
                        .map(|value| value.trim().to_ascii_lowercase())
                        .collect::<Vec<_>>();
                    assert_eq!(
                        vary.iter()
                            .filter(|value| *value == "accept-encoding")
                            .count(),
                        1,
                        "{path}, {head}, {encoding:?}"
                    );
                    assert!(vary.iter().any(|value| value == "origin"));
                    assert_eq!(
                        response.headers().contains_key(CONTENT_ENCODING),
                        encoding == Some("gzip")
                    );
                    if head {
                        assert!(response.take_bytes(None).await.unwrap().is_empty());
                    } else {
                        assert_eq!(response.take_string().await.unwrap(), "hello");
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn ineligible_responses_do_not_gain_a_vary_dimension() {
    for compression in [
        Compression::new().min_length(100),
        Compression::new().min_length(1).disable_all(),
        Compression::new()
            .min_length(1)
            .content_types(&["application/json".parse().unwrap()]),
    ] {
        let service = Service::new(Router::with_hoop(compression).goal(representation));
        for encoding in ["identity", "gzip"] {
            let response = TestClient::get("http://localhost/")
                .add_header("accept-encoding", encoding, true)
                .send(&service)
                .await;
            assert_eq!(response.headers()[VARY], "origin");
            assert!(!response.headers().contains_key(CONTENT_ENCODING));
        }
    }
}
