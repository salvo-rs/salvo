#![allow(missing_docs)]

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use salvo_core::http::{ReqBody, ResBody};
use salvo_core::prelude::*;
use salvo_core::test::TestClient;
use salvo_proxy::{Client, Proxy};

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

struct EchoUpstream;
impl Client for EchoUpstream {
    type Error = Infallible;
    async fn execute(
        &self,
        req: hyper::Request<ReqBody>,
        _: Option<hyper::upgrade::OnUpgrade>,
    ) -> Result<hyper::Response<ResBody>, Self::Error> {
        Ok(hyper::Response::new(ResBody::from(
            req.uri().path().to_owned(),
        )))
    }
}

fn protected_service(calls: Arc<AtomicUsize>) -> Service {
    Service::new(
        Router::new()
            .push(
                Router::with_path("report_note/{**rest}")
                    .hoop(Deny(calls))
                    .goal(Proxy::new(
                        "http://upstream.invalid/report_note",
                        EchoUpstream,
                    )),
            )
            .push(
                Router::with_path("{**rest}")
                    .goal(Proxy::new("http://upstream.invalid", EchoUpstream)),
            ),
    )
}

fn request(path: &str) -> Request {
    let mut request = TestClient::get("http://localhost/").build();
    *request.uri_mut() = format!("http://localhost{path}").parse().unwrap();
    request
}

#[tokio::test]
async fn path_separators_are_rejected_before_proxy_routing() {
    let calls = Arc::new(AtomicUsize::new(0));
    let proxy = Proxy::new("http://upstream.invalid", EchoUpstream);
    assert!(proxy.strict_path_normalization_enabled);
    let service = protected_service(calls.clone());
    for (path, expected, guard_calls) in [
        ("/report_note/secret.txt", StatusCode::FORBIDDEN, 1),
        ("/%72eport_note/secret.txt", StatusCode::FORBIDDEN, 1),
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
        let response = service.handle(request(path)).await;
        assert_eq!(response.status_code, Some(expected), "{path}");
        assert_eq!(calls.load(Ordering::SeqCst), guard_calls, "{path}");
    }
}

#[tokio::test]
async fn dot_segments_use_the_protected_proxy_route() {
    let calls = Arc::new(AtomicUsize::new(0));
    let service = protected_service(calls.clone());
    for path in [
        "/./report_note/secret.txt",
        "/%2e/report_note/secret.txt",
        "/%2E/report_note/secret.txt",
        "/public/../report_note/secret.txt",
        "/public/%2e%2e/report_note/secret.txt",
        "/public/%2E%2E/report_note/secret.txt",
    ] {
        calls.store(0, Ordering::SeqCst);
        let response = service.handle(request(path)).await;
        assert_eq!(response.status_code, Some(StatusCode::FORBIDDEN), "{path}");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "{path}");
    }
}

#[tokio::test]
async fn discarded_path_segments_are_rejected_before_proxy_routing() {
    let calls = Arc::new(AtomicUsize::new(0));
    let service = protected_service(calls.clone());
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
        #[cfg(windows)]
        "/CON/../report_note/secret.txt",
    ] {
        calls.store(0, Ordering::SeqCst);
        let response = service.handle(request(path)).await;
        assert_eq!(
            response.status_code,
            Some(StatusCode::BAD_REQUEST),
            "{path}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0, "{path}");
    }
}

#[tokio::test]
async fn safe_encoded_paths_still_reach_public_upstream() {
    use salvo_core::test::ResponseExt;

    let calls = Arc::new(AtomicUsize::new(0));
    let service = protected_service(calls.clone());
    for (path, expected) in [
        ("/hello%20world.txt", "/hello%20world.txt"),
        ("/v1%2e2.txt", "/v1.2.txt"),
        ("/CONSOLE.txt", "/CONSOLE.txt"),
        ("/COM10.txt", "/COM10.txt"),
    ] {
        let mut response = service.handle(request(path)).await;
        assert_eq!(response.status_code, Some(StatusCode::OK), "{path}");
        assert_eq!(response.take_string().await.unwrap(), expected, "{path}");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
