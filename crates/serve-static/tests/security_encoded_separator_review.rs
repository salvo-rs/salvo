#![allow(missing_docs)]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_serve_static::StaticDir;

struct Deny(Arc<AtomicUsize>);
#[async_trait]
impl Handler for Deny {
    async fn handle(
        &self,
        _: &mut Request,
        _: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        self.0.fetch_add(1, Ordering::SeqCst);
        res.status_code(StatusCode::FORBIDDEN);
        ctrl.skip_rest();
    }
}

fn protected_service(root: &Path, calls: Arc<AtomicUsize>) -> Service {
    Service::new(
        Router::new()
            .push(
                Router::with_path("report_note/{**rest}")
                    .hoop(Deny(calls))
                    .goal(StaticDir::new(root.join("report_note"))),
            )
            .push(Router::with_path("{**rest}").goal(StaticDir::new(root.to_path_buf()))),
    )
}

fn request(path: &str) -> Request {
    let mut request = TestClient::get("http://localhost/").build();
    *request.uri_mut() = format!("http://localhost{path}").parse().unwrap();
    request
}

#[tokio::test]
async fn embedded_separators_are_rejected_by_static_resources() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("report_note")).unwrap();
    std::fs::write(
        root.path().join("report_note/secret.txt"),
        "PRIVATE_SENTINEL",
    )
    .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let service = protected_service(root.path(), calls.clone());
    for (path, expected, guard_calls) in [
        ("/report_note/secret.txt", StatusCode::FORBIDDEN, 1),
        ("/%72eport_note/secret.txt", StatusCode::FORBIDDEN, 1),
        #[cfg(windows)]
        ("/REPORT_NOTE/secret.txt", StatusCode::NOT_FOUND, 0),
        #[cfg(windows)]
        ("/%52eport_note/secret.txt", StatusCode::NOT_FOUND, 0),
        #[cfg(windows)]
        ("/report_note./secret.txt", StatusCode::NOT_FOUND, 0),
        #[cfg(windows)]
        ("/report_note%2e/secret.txt", StatusCode::NOT_FOUND, 0),
        ("/%2freport_note/secret.txt", StatusCode::BAD_REQUEST, 0),
        ("/%2Freport_note/secret.txt", StatusCode::BAD_REQUEST, 0),
        ("/%5creport_note/secret.txt", StatusCode::BAD_REQUEST, 0),
        ("/%5Creport_note/secret.txt", StatusCode::BAD_REQUEST, 0),
        ("/report_note%2fsecret.txt", StatusCode::BAD_REQUEST, 0),
        ("/report_note%5csecret.txt", StatusCode::BAD_REQUEST, 0),
        ("/\\report_note/secret.txt", StatusCode::BAD_REQUEST, 0),
        ("/report_note\\secret.txt", StatusCode::BAD_REQUEST, 0),
    ] {
        calls.store(0, Ordering::SeqCst);
        let mut response = service.handle(request(path)).await;
        assert_eq!(response.status_code, Some(expected), "{path}");
        assert_eq!(calls.load(Ordering::SeqCst), guard_calls, "{path}");
        assert!(
            !response
                .take_string()
                .await
                .unwrap()
                .contains("PRIVATE_SENTINEL"),
            "{path}"
        );
    }
}

#[tokio::test]
async fn dot_segments_are_normalized_before_routing() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("report_note")).unwrap();
    std::fs::write(
        root.path().join("report_note/secret.txt"),
        "PRIVATE_SENTINEL",
    )
    .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let service = protected_service(root.path(), calls.clone());
    for path in [
        "/./report_note/secret.txt",
        "/%2e/report_note/secret.txt",
        "/%2E/report_note/secret.txt",
        "/public/../report_note/secret.txt",
        "/public/%2e%2e/report_note/secret.txt",
        "/public/%2E%2E/report_note/secret.txt",
        "/CON/../report_note/secret.txt",
    ] {
        calls.store(0, Ordering::SeqCst);
        let mut response = service.handle(request(path)).await;
        assert_eq!(response.status_code, Some(StatusCode::FORBIDDEN), "{path}");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "{path}");
        assert!(
            !response
                .take_string()
                .await
                .unwrap()
                .contains("PRIVATE_SENTINEL"),
            "{path}"
        );
    }
}

#[tokio::test]
async fn discarded_path_segments_are_rejected_before_static_routing() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("report_note")).unwrap();
    std::fs::write(
        root.path().join("report_note/secret.txt"),
        "PRIVATE_SENTINEL",
    )
    .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let service = protected_service(root.path(), calls.clone());
    for path in [
        "/%00/report_note/secret.txt",
        "/ignored%00segment/report_note/secret.txt",
        "/%00/../report_note/secret.txt",
        "/report_note/secret.txt%00",
        #[cfg(windows)]
        "/C:/report_note/secret.txt",
        #[cfg(windows)]
        "/C%3a/report_note/secret.txt",
        #[cfg(windows)]
        "/CON/report_note/secret.txt",
        #[cfg(windows)]
        "/%43%4f%4e/report_note/secret.txt",
        #[cfg(windows)]
        "/NUL.txt/report_note/secret.txt",
        #[cfg(windows)]
        "/pRn/report_note/secret.txt",
        #[cfg(windows)]
        "/COM1/report_note/secret.txt",
        #[cfg(windows)]
        "/LPT9/report_note/secret.txt",
    ] {
        calls.store(0, Ordering::SeqCst);
        let mut response = service.handle(request(path)).await;
        assert_eq!(
            response.status_code,
            Some(StatusCode::BAD_REQUEST),
            "{path}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0, "{path}");
        assert!(
            !response
                .take_string()
                .await
                .unwrap()
                .contains("PRIVATE_SENTINEL"),
            "{path}"
        );
    }
}

#[tokio::test]
async fn safe_encoded_paths_still_serve_public_files() {
    let root = tempfile::tempdir().unwrap();
    for file in [
        "hello world.txt",
        "v1.2.txt",
        "%2f.txt",
        "CONSOLE.txt",
        "COM10.txt",
    ] {
        std::fs::write(root.path().join(file), "PUBLIC_SENTINEL").unwrap();
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let service = protected_service(root.path(), calls.clone());
    for path in [
        "/hello%20world.txt",
        "/v1%2e2.txt",
        "/%252f.txt",
        "/CONSOLE.txt",
        "/COM10.txt",
    ] {
        let mut response = service.handle(request(path)).await;
        assert_eq!(response.status_code, Some(StatusCode::OK), "{path}");
        assert_eq!(
            response.take_string().await.unwrap(),
            "PUBLIC_SENTINEL",
            "{path}"
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn invalid_segments_never_reach_static_defaults_fallbacks_or_alias_compatibility() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("report_note")).unwrap();
    std::fs::write(
        root.path().join("report_note/secret.txt"),
        "PRIVATE_SENTINEL",
    )
    .unwrap();
    std::fs::write(root.path().join("public.txt"), "FALLBACK_SENTINEL").unwrap();
    for pattern in ["{**rest}", "prefix{**rest}", "{name}"] {
        let service = Service::new(
            Router::with_path(pattern).goal(
                StaticDir::new(root.path().to_path_buf())
                    .strict_path_check(false)
                    .defaults("public.txt")
                    .fallback("public.txt"),
            ),
        );
        for encoded in ["%2f", "%2F", "%5c", "%5C"] {
            let prefix = if pattern.starts_with("prefix") {
                "prefix"
            } else {
                ""
            };
            let target = format!("/{prefix}report_note{encoded}secret.txt");
            for method in ["GET", "HEAD"] {
                let mut req = request(&target);
                *req.method_mut() = method.parse().unwrap();
                req.headers_mut()
                    .insert("range", "bytes=0-1".parse().unwrap());
                req.headers_mut()
                    .insert("if-none-match", "*".parse().unwrap());
                let mut response = service.handle(req).await;
                assert_eq!(
                    response.status_code,
                    Some(StatusCode::BAD_REQUEST),
                    "{pattern}: {target}"
                );
                let body = response.take_string().await.unwrap();
                assert!(!body.contains("PRIVATE_SENTINEL") && !body.contains("FALLBACK_SENTINEL"));
                assert!(!response.headers().contains_key("etag"));
                assert!(!response.headers().contains_key("last-modified"));
            }
        }
    }
}

#[cfg(feature = "embed")]
#[tokio::test]
async fn embedded_resources_keep_the_same_separator_boundary() {
    use rust_embed::RustEmbed;
    use salvo_serve_static::StaticEmbed;
    #[derive(RustEmbed)]
    #[folder = "test/static/"]
    struct Assets;
    let guards = Arc::new(AtomicUsize::new(0));
    let service = Service::new(
        Router::new()
            .push(
                Router::with_path("dir1/{**rest}")
                    .hoop(Deny(guards.clone()))
                    .goal(StaticEmbed::<Assets>::new()),
            )
            .push(
                Router::with_path("{**rest}")
                    .goal(StaticEmbed::<Assets>::new().fallback("index.html")),
            ),
    );
    for path in [
        "/%2fdir1/test3.txt",
        "/%2Fdir1/test3.txt",
        "/%5cdir1/test3.txt",
        "/%5Cdir1/test3.txt",
        "/dir1%2Ftest3.txt",
    ] {
        assert_eq!(
            service.handle(request(path)).await.status_code,
            Some(StatusCode::BAD_REQUEST),
            "{path}"
        );
    }
    assert_eq!(guards.load(Ordering::SeqCst), 0);
    assert_eq!(
        service.handle(request("/dir1/test3.txt")).await.status_code,
        Some(StatusCode::FORBIDDEN)
    );
    assert_eq!(guards.load(Ordering::SeqCst), 1);
}
