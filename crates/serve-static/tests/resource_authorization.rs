//! Authorization applies to resolved resources before static response handling.
#![allow(missing_docs, clippy::unwrap_used)]

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use flate2::Compression;
use flate2::write::GzEncoder;
use salvo_core::http::{Method, StatusCode};
use salvo_core::test::{RequestBuilder, ResponseExt, TestClient};
use salvo_core::{Depot, Request, Response, Router, Service, handler};
use salvo_serve_static::{StaticDir, StaticResource, StaticResourceAuthorizer};

fn service(handler: StaticDir) -> Service {
    Service::new(Router::with_path("{**path}").goal(handler))
}

#[tokio::test]
async fn response_keeps_the_handle_opened_before_async_authorization() {
    struct RemoveAfterOpen;
    #[salvo_core::async_trait]
    impl StaticResourceAuthorizer for RemoveAfterOpen {
        async fn authorize(&self, _: &Request, _: &Depot, resource: &StaticResource) -> bool {
            tokio::task::yield_now().await;
            if resource.is_file() {
                fs::remove_file(resource.path()).unwrap();
            }
            true
        }
    }

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("asset.txt");
    fs::write(&path, "authorized handle bytes").unwrap();
    let handler = StaticDir::new(directory.path().to_path_buf())
        .preload_threshold(0)
        .authorize(RemoveAfterOpen);
    let mut response = TestClient::get("http://localhost/asset.txt")
        .send(&service(handler))
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(
        response.take_string().await.unwrap(),
        "authorized handle bytes"
    );
    assert!(!path.exists());
}

async fn assert_denied(mut response: Response, private_contents: &[&str]) {
    assert_eq!(response.status_code, Some(StatusCode::NOT_FOUND));
    for header in [
        "etag",
        "last-modified",
        "accept-ranges",
        "content-range",
        "content-encoding",
        "location",
    ] {
        assert!(
            !response.headers().contains_key(header),
            "denial exposed {header}"
        );
    }
    let body = response.take_string().await.unwrap();
    for private_content in private_contents {
        assert!(
            !body.contains(private_content),
            "denial exposed private content: {private_content}"
        );
    }
}

#[handler]
async fn identify(depot: &mut Depot) {
    depot.insert("subject", "reader");
}

#[handler]
async fn must_not_run() {
    panic!("authorization denial continued to the next handler");
}

#[tokio::test]
async fn synchronous_policy_receives_request_depot_and_canonical_resource() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("nested")).unwrap();
    fs::write(directory.path().join("nested/asset.txt"), "public asset").unwrap();
    let expected_root = directory.path().canonicalize().unwrap();
    let expected_path = directory
        .path()
        .join("nested/asset.txt")
        .canonicalize()
        .unwrap();
    let observations = Arc::new(AtomicUsize::new(0));
    let recorded = Arc::clone(&observations);
    let static_dir = StaticDir::new(directory.path().join(".")).authorize(
        move |req: &Request, depot: &Depot, resource: &StaticResource| {
            assert_eq!(resource.root(), expected_root);
            assert_eq!(
                resource.path(),
                resource.root().join(resource.relative_path())
            );
            assert_eq!(resource.is_dir(), resource.metadata().is_dir());
            assert_eq!(resource.is_file(), resource.metadata().is_file());
            if resource.is_file() {
                assert_eq!(resource.path(), expected_path);
                assert_eq!(resource.relative_path(), Path::new("nested/asset.txt"));
                assert_eq!(resource.metadata().len(), 12);
                recorded.fetch_add(1, Ordering::SeqCst);
            }
            req.headers()
                .get("x-reader")
                .is_some_and(|value| value == "yes")
                && depot.get::<&str>("subject").ok().copied() == Some("reader")
        },
    );
    let service = Service::new(
        Router::with_path("{**path}")
            .hoop(identify)
            .goal(static_dir),
    );
    let mut allowed = TestClient::get("http://localhost/nested/asset.txt")
        .add_header("x-reader", "yes", true)
        .send(&service)
        .await;
    assert_eq!(allowed.status_code, Some(StatusCode::OK));
    assert_eq!(allowed.take_string().await.unwrap(), "public asset");
    assert!(observations.load(Ordering::SeqCst) > 0);
    let denied = TestClient::get("http://localhost/nested/asset.txt")
        .send(&service)
        .await;
    assert_denied(denied, &["public asset"]).await;
}

struct AsyncReader;

#[salvo_core::async_trait]
impl StaticResourceAuthorizer for AsyncReader {
    async fn authorize(&self, req: &Request, depot: &Depot, resource: &StaticResource) -> bool {
        tokio::task::yield_now().await;
        resource.is_file()
            && resource.relative_path() == Path::new("asset.txt")
            && req
                .headers()
                .get("x-reader")
                .is_some_and(|value| value == "yes")
            && depot.get::<&str>("subject").ok().copied() == Some("reader")
    }
}

#[tokio::test]
async fn asynchronous_policy_can_await_and_read_request_context() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("asset.txt"),
        "async-authorized content",
    )
    .unwrap();
    let service = Service::new(
        Router::with_path("{**path}")
            .hoop(identify)
            .goal(StaticDir::new(directory.path().to_path_buf()).authorize(AsyncReader)),
    );
    let mut allowed = TestClient::get("http://localhost/asset.txt")
        .add_header("x-reader", "yes", true)
        .send(&service)
        .await;
    assert_eq!(allowed.status_code, Some(StatusCode::OK));
    assert_eq!(
        allowed.take_string().await.unwrap(),
        "async-authorized content"
    );
    let denied = TestClient::get("http://localhost/asset.txt")
        .send(&service)
        .await;
    assert_denied(denied, &["async-authorized content"]).await;
}

#[tokio::test]
async fn authorization_preserves_successful_head_range_and_validator_responses() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("asset.txt"), "public file content").unwrap();
    let service = service(
        StaticDir::new(directory.path().to_path_buf())
            .authorize(|_: &Request, _: &Depot, _: &StaticResource| true),
    );
    let mut full = TestClient::get("http://localhost/asset.txt")
        .send(&service)
        .await;
    assert_eq!(full.status_code, Some(StatusCode::OK));
    assert_eq!(full.headers()["accept-ranges"], "bytes");
    let etag = full.headers()["etag"].clone();
    let modified = full.headers()["last-modified"].clone();
    assert_eq!(full.take_string().await.unwrap(), "public file content");

    let mut head = TestClient::head("http://localhost/asset.txt")
        .add_header("range", "bytes=0-3", true)
        .send(&service)
        .await;
    assert_eq!(head.status_code, Some(StatusCode::OK));
    assert_eq!(head.headers()["etag"], etag);
    assert!(head.take_bytes(None).await.unwrap().is_empty());
    let mut range = TestClient::get("http://localhost/asset.txt")
        .add_header("range", "bytes=0-3", true)
        .send(&service)
        .await;
    assert_eq!(range.status_code, Some(StatusCode::PARTIAL_CONTENT));
    assert_eq!(range.headers()["content-range"], "bytes 0-3/19");
    assert_eq!(range.take_string().await.unwrap(), "publ");
    for method in [Method::GET, Method::HEAD] {
        for (header, validator) in [
            ("if-none-match", etag.clone()),
            ("if-modified-since", modified.clone()),
        ] {
            let mut unchanged = RequestBuilder::new("http://localhost/asset.txt", method.clone())
                .add_header(header, validator, true)
                .send(&service)
                .await;
            assert_eq!(unchanged.status_code, Some(StatusCode::NOT_MODIFIED));
            assert!(unchanged.take_bytes(None).await.unwrap().is_empty());
        }
    }
}

#[tokio::test]
async fn denial_precedes_methods_ranges_and_conditional_metadata() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("private.txt"),
        "classified file payload",
    )
    .unwrap();
    let public = service(StaticDir::new(directory.path().to_path_buf()));
    let original = TestClient::get("http://localhost/private.txt")
        .send(&public)
        .await;
    let etag = original.headers()["etag"].to_str().unwrap().to_owned();
    let modified = original.headers()["last-modified"]
        .to_str()
        .unwrap()
        .to_owned();
    let denied = service(
        StaticDir::new(directory.path().to_path_buf())
            .authorize(|_: &Request, _: &Depot, _: &StaticResource| false),
    );
    for method in [
        Method::GET,
        Method::HEAD,
        Method::POST,
        Method::PUT,
        Method::PATCH,
        Method::DELETE,
        Method::OPTIONS,
        Method::from_bytes(b"REPORT").unwrap(),
    ] {
        for condition in [
            None,
            Some(("range", "bytes=0-3")),
            Some(("range", "bytes=999999-")),
            Some(("if-none-match", etag.as_str())),
            Some(("if-none-match", "*")),
            Some(("if-modified-since", modified.as_str())),
            Some(("if-match", "\"different\"")),
        ] {
            let mut request = RequestBuilder::new("http://localhost/private.txt", method.clone());
            if let Some((name, value)) = condition {
                request = request.add_header(name, value, true);
            }
            assert_denied(request.send(&denied).await, &["classified file payload"]).await;
        }
    }
}

#[tokio::test]
async fn denied_first_root_is_terminal_without_fallback_or_next_handler() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first");
    let later = directory.path().join("later");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&later).unwrap();
    fs::write(first.join("private.txt"), "first root private payload").unwrap();
    fs::write(later.join("private.txt"), "later root public payload").unwrap();
    fs::write(first.join("fallback.txt"), "public fallback payload").unwrap();
    let denied_root = first.canonicalize().unwrap();
    let handler = StaticDir::new(vec![first, later])
        .fallback("fallback.txt")
        .authorize(move |_: &Request, _: &Depot, resource: &StaticResource| {
            resource.root() != denied_root || resource.relative_path() != Path::new("private.txt")
        });
    let service = Service::new(
        Router::with_path("{**path}")
            .hoop(handler)
            .goal(must_not_run),
    );
    let response = TestClient::get("http://localhost/private.txt")
        .send(&service)
        .await;
    assert_denied(
        response,
        &[
            "first root private payload",
            "later root public payload",
            "public fallback payload",
        ],
    )
    .await;
}

#[tokio::test]
async fn default_file_policy_checks_actual_path_and_does_not_try_another_default() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("folder")).unwrap();
    fs::write(
        directory.path().join("folder/index.html"),
        "private default payload",
    )
    .unwrap();
    fs::write(
        directory.path().join("folder/home.html"),
        "other default payload",
    )
    .unwrap();
    fs::write(directory.path().join("fallback.html"), "fallback payload").unwrap();
    let service = service(
        StaticDir::new(directory.path().to_path_buf())
            .defaults(["index.html", "home.html"])
            .fallback("fallback.html")
            .auto_list(true)
            .authorize(|_: &Request, _: &Depot, resource: &StaticResource| {
                resource.relative_path() != Path::new("folder/index.html")
            }),
    );
    let response = TestClient::get("http://localhost/folder/")
        .send(&service)
        .await;
    assert_denied(
        response,
        &[
            "private default payload",
            "other default payload",
            "fallback payload",
        ],
    )
    .await;
}

#[tokio::test]
async fn fallback_policy_checks_actual_path_and_denial_is_terminal() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first");
    let later = directory.path().join("later");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&later).unwrap();
    fs::write(first.join("fallback.html"), "private fallback payload").unwrap();
    fs::write(later.join("fallback.html"), "later fallback payload").unwrap();
    let first_root = first.canonicalize().unwrap();
    let service = service(
        StaticDir::new(vec![first, later])
            .fallback("fallback.html")
            .authorize(move |_: &Request, _: &Depot, resource: &StaticResource| {
                resource.root() != first_root
                    || resource.relative_path() != Path::new("fallback.html")
            }),
    );
    for target in ["missing.txt", "nested/missing.txt"] {
        let response = TestClient::get(format!("http://localhost/{target}"))
            .send(&service)
            .await;
        assert_denied(
            response,
            &["private fallback payload", "later fallback payload"],
        )
        .await;
    }
}

#[tokio::test]
async fn denied_directory_never_redirects_or_lists_even_with_public_index() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("private")).unwrap();
    fs::write(
        directory.path().join("private/index.html"),
        "directory index payload",
    )
    .unwrap();
    fs::write(
        directory.path().join("fallback.html"),
        "public fallback payload",
    )
    .unwrap();
    let service = service(
        StaticDir::new(directory.path().to_path_buf())
            .defaults("index.html")
            .fallback("fallback.html")
            .auto_list(true)
            .authorize(|_: &Request, _: &Depot, resource: &StaticResource| {
                resource.relative_path() != Path::new("private")
            }),
    );
    for target in ["private?next=1", "private/"] {
        let response = TestClient::get(format!("http://localhost/{target}"))
            .send(&service)
            .await;
        assert_denied(
            response,
            &["directory index payload", "public fallback payload"],
        )
        .await;
    }
}

#[tokio::test]
async fn listings_only_show_authorized_files_and_directories_in_every_format() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("visible-file.txt"),
        "visible contents",
    )
    .unwrap();
    fs::write(directory.path().join("hidden-file.txt"), "private contents").unwrap();
    fs::create_dir(directory.path().join("visible-folder")).unwrap();
    fs::create_dir(directory.path().join("hidden-folder")).unwrap();
    fs::write(
        directory.path().join("hidden-folder/secret.txt"),
        "nested private contents",
    )
    .unwrap();
    let service = service(
        StaticDir::new(directory.path().to_path_buf())
            .auto_list(true)
            .authorize(|_: &Request, _: &Depot, resource: &StaticResource| {
                !resource.relative_path().components().any(|component| {
                    component.as_os_str() == "hidden-file.txt"
                        || component.as_os_str() == "hidden-folder"
                })
            }),
    );
    for format in ["text/plain", "text/html", "application/json", "text/xml"] {
        let mut response = TestClient::get("http://localhost/")
            .add_header("accept", format, true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK), "{format}");
        let listing = response.take_string().await.unwrap();
        assert!(listing.contains("visible-file.txt"), "{format}: {listing}");
        assert!(listing.contains("visible-folder"), "{format}: {listing}");
        assert!(!listing.contains("hidden-file.txt"), "{format}: {listing}");
        assert!(!listing.contains("hidden-folder"), "{format}: {listing}");
    }
    for target in [
        "hidden-file.txt",
        "hidden-folder/",
        "hidden-folder/secret.txt",
    ] {
        let response = TestClient::get(format!("http://localhost/{target}"))
            .send(&service)
            .await;
        assert_denied(
            response,
            &["private contents", "nested private contents", "secret.txt"],
        )
        .await;
    }
}

#[tokio::test]
async fn logical_file_and_selected_compressed_sidecar_both_require_authorization() {
    let directory = tempfile::tempdir().unwrap();
    let contents = "private compressed payload";
    fs::write(directory.path().join("asset.txt"), contents).unwrap();
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(contents.as_bytes()).unwrap();
    fs::write(
        directory.path().join("asset.txt.gz"),
        encoder.finish().unwrap(),
    )
    .unwrap();

    let allowed = service(
        StaticDir::new(directory.path().to_path_buf())
            .authorize(|_: &Request, _: &Depot, _: &StaticResource| true),
    );
    let mut compressed = TestClient::get("http://localhost/asset.txt")
        .add_header("accept-encoding", "gzip", true)
        .send(&allowed)
        .await;
    assert_eq!(compressed.status_code, Some(StatusCode::OK));
    assert_eq!(compressed.headers()["content-encoding"], "gzip");
    assert_eq!(compressed.take_string().await.unwrap(), contents);

    for denied_path in ["asset.txt", "asset.txt.gz"] {
        let service = service(StaticDir::new(directory.path().to_path_buf()).authorize(
            move |_: &Request, _: &Depot, resource: &StaticResource| {
                resource.relative_path() != Path::new(denied_path)
            },
        ));
        for encoding in ["gzip", "gzip, identity;q=0"] {
            let response = TestClient::get("http://localhost/asset.txt")
                .add_header("accept-encoding", encoding, true)
                .send(&service)
                .await;
            assert_denied(response, &[contents]).await;
        }
    }

    let service = service(StaticDir::new(directory.path().to_path_buf()).authorize(
        |_: &Request, _: &Depot, resource: &StaticResource| {
            resource.relative_path() != Path::new("asset.txt.gz")
        },
    ));
    let mut identity = TestClient::get("http://localhost/asset.txt")
        .add_header("accept-encoding", "identity", true)
        .send(&service)
        .await;
    assert_eq!(identity.status_code, Some(StatusCode::OK));
    assert!(!identity.headers().contains_key("content-encoding"));
    assert_eq!(identity.take_string().await.unwrap(), contents);
}
