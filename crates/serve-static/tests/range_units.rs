use salvo_core::http::StatusCode;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_core::writing::ReadSeeker;
use salvo_core::{Request, Response, Router, Service, handler};
use salvo_serve_static::{StaticDir, StaticFile};

async fn check_range_units(service: &Service) {
    let mut original = TestClient::get("http://localhost/test1.txt")
        .send(service)
        .await;
    let etag = original.headers()["etag"].clone();
    let body = original.take_bytes(None).await.unwrap();
    for range in ["items=0-1", "records=999999-", "custom=opaque"] {
        let mut response = TestClient::get("http://localhost/test1.txt")
            .add_header("range", range, true)
            .send(service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK), "{range}");
        assert!(!response.headers().contains_key("content-range"));
        assert_eq!(response.headers()["content-length"], body.len().to_string());
        assert_eq!(response.take_bytes(None).await.unwrap(), body);

        let response = TestClient::get("http://localhost/test1.txt")
            .add_header("range", range, true)
            .add_header("if-none-match", etag.clone(), true)
            .send(service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::NOT_MODIFIED));
    }
    for range in ["bytes=0-1", "BYTES=0-1", "bYtEs=0-1"] {
        let mut response = TestClient::get("http://localhost/test1.txt")
            .add_header("range", range, true)
            .send(service)
            .await;
        assert_eq!(
            response.status_code,
            Some(StatusCode::PARTIAL_CONTENT),
            "{range}"
        );
        assert_eq!(
            response.headers()["content-range"],
            format!("bytes 0-1/{}", body.len())
        );
        assert_eq!(response.take_bytes(None).await.unwrap(), body.slice(..2));
    }
    for range in ["bytes=999999-", "BYTES=999999-", "bytes=bad"] {
        let response = TestClient::get("http://localhost/test1.txt")
            .add_header("range", range, true)
            .send(service)
            .await;
        assert_eq!(
            response.status_code,
            Some(StatusCode::RANGE_NOT_SATISFIABLE),
            "{range}"
        );
    }
}

#[tokio::test]
async fn named_file_range_units() {
    check_range_units(&Service::new(
        Router::with_path("test1.txt").goal(StaticFile::new("test/static/test1.txt")),
    ))
    .await;
}

#[tokio::test]
async fn static_dir_range_units() {
    check_range_units(&Service::new(
        Router::with_path("{*path}").goal(StaticDir::new(["test/static"])),
    ))
    .await;
}

#[handler]
async fn reader(req: &mut Request, res: &mut Response) {
    ReadSeeker::new(std::io::Cursor::new(b"abcdef".to_vec()), 6)
        .etag("\"version\"".parse().unwrap())
        .send(req.headers(), res)
        .await;
}

#[tokio::test]
async fn read_seeker_range_units() {
    check_range_units(&Service::new(Router::with_path("test1.txt").goal(reader))).await;
}

#[cfg(feature = "embed")]
#[tokio::test]
async fn static_embed_range_units() {
    #[derive(rust_embed::RustEmbed)]
    #[folder = "test/static/"]
    struct Assets;
    check_range_units(&Service::new(
        Router::with_path("{*path}").goal(salvo_serve_static::StaticEmbed::<Assets>::new()),
    ))
    .await;
}
