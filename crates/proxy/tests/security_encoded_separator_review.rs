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
async fn embedded_separators_are_rejected_before_proxy_forwarding() {
    let calls = Arc::new(AtomicUsize::new(0));
    let proxy = Proxy::new("http://upstream.invalid", EchoUpstream);
    assert!(proxy.strict_path_normalization_enabled);
    let service = protected_service(calls.clone());
    for (path, expected, guard_calls) in [
        ("/report_note/secret.txt", StatusCode::FORBIDDEN, 1),
        ("/%72eport_note/secret.txt", StatusCode::FORBIDDEN, 1),
        // Decoded once by the router, this is `%72eport_note`, which an
        // upstream that decodes again would read as the protected prefix.
        ("/%2572eport_note/secret.txt", StatusCode::BAD_REQUEST, 0),
        ("/%2552eport_note/secret.txt", StatusCode::BAD_REQUEST, 0),
        ("/report%255fnote/secret.txt", StatusCode::BAD_REQUEST, 0),
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
        "/CON/../report_note/secret.txt",
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

async fn forwarded_path(service: &Service, path: &str) -> (Option<StatusCode>, String) {
    use salvo_core::test::ResponseExt;

    let mut response = service.handle(request(path)).await;
    let status = response.status_code;
    (status, response.take_string().await.unwrap_or_default())
}

#[tokio::test]
async fn literal_percent_signs_reach_the_upstream_escaped() {
    let service = protected_service(Arc::new(AtomicUsize::new(0)));
    for path in ["/files/50%25off.txt", "/files/100%25", "/files/a%25/b%25z"] {
        assert_eq!(
            forwarded_path(&service, path).await,
            (Some(StatusCode::OK), path.to_owned()),
            "{path}"
        );
    }
    // After the default getter escapes the routed percent sign, the strict
    // check sees `%25` followed by two hex digits. This includes controls as
    // well as unreserved characters; none may become an upstream escape.
    for path in [
        "/files/a%2541.txt",
        "/files/a%25%34%31.txt",
        "/api/x%2500y",
        "/api/x%250dy",
        "/api/x%250ay",
        "/api/x%250d%250ay",
        "/api/x%257fy",
        "/%252572eport_note/secret.txt",
    ] {
        assert_eq!(
            forwarded_path(&service, path).await.0,
            Some(StatusCode::BAD_REQUEST),
            "{path}"
        );
    }

    let lenient = Service::new(Router::with_path("{**rest}").goal(
        Proxy::new("http://upstream.invalid", EchoUpstream).strict_path_normalization(false),
    ));
    for path in [
        "/files/a%2541.txt",
        "/%252e%252e/secret.txt",
        "/api/x%2500y",
        "/api/x%250d%250ay",
    ] {
        assert_eq!(
            forwarded_path(&lenient, path).await,
            (Some(StatusCode::OK), path.to_owned()),
            "{path}"
        );
    }
}

#[tokio::test]
async fn matrix_path_data_is_reencoded_per_segment_without_mutating_the_request_uri() {
    #[handler]
    async fn unchanged(
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let uri = req.uri().clone();
        ctrl.call_next(req, depot, res).await;
        assert_eq!(req.uri(), &uri);
    }
    let service = Service::new(Router::with_path("{**rest}").hoop(unchanged).goal(
        Proxy::new("http://upstream.invalid", EchoUpstream).strict_path_normalization(false),
    ));
    for (target, expected) in [
        (
            "/_matrix/profile/@alice:example.org",
            "/_matrix/profile/@alice:example.org",
        ),
        (
            "/_matrix/profile/@a%2fb%3Aexample.org",
            "/_matrix/profile/@a%2Fb:example.org",
        ),
        (
            "/_matrix/profile/@a%252Fb:example.org",
            "/_matrix/profile/@a%252Fb:example.org",
        ),
        (
            "/_matrix/profile/@a%5Cb:example.org",
            "/_matrix/profile/@a%5Cb:example.org",
        ),
        ("/CON/C%3A/file", "/CON/C:/file"),
    ] {
        assert_eq!(
            forwarded_path(&service, target).await,
            (Some(StatusCode::OK), expected.to_owned()),
            "{target}"
        );
    }
}

struct CountUpstream(Arc<AtomicUsize>);
impl Client for CountUpstream {
    type Error = Infallible;
    async fn execute(
        &self,
        _: hyper::Request<ReqBody>,
        _: Option<hyper::upgrade::OnUpgrade>,
    ) -> Result<hyper::Response<ResBody>, Self::Error> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(hyper::Response::new(ResBody::from("PRIVATE_SENTINEL")))
    }
}

#[tokio::test]
async fn default_proxy_never_contacts_upstream_for_encoded_separators_or_nested_escapes() {
    use salvo_core::test::ResponseExt;
    let calls = Arc::new(AtomicUsize::new(0));
    let service = Service::new(Router::with_path("{**rest}").goal(Proxy::new(
        "http://upstream.invalid",
        CountUpstream(calls.clone()),
    )));
    for path in [
        "/%2freport_note/secret.txt",
        "/%2Freport_note/secret.txt",
        "/%5creport_note/secret.txt",
        "/%5Creport_note/secret.txt",
        "/%252freport_note/secret.txt",
        "/%255creport_note/secret.txt",
        "/%2572eport_note/secret.txt",
    ] {
        for method in ["GET", "HEAD"] {
            let mut req = request(path);
            *req.method_mut() = method.parse().unwrap();
            req.headers_mut()
                .insert("range", "bytes=0-1".parse().unwrap());
            req.headers_mut()
                .insert("if-none-match", "*".parse().unwrap());
            let mut response = service.handle(req).await;
            assert_eq!(
                response.status_code,
                Some(StatusCode::BAD_REQUEST),
                "{method} {path}"
            );
            assert!(
                !response
                    .take_string()
                    .await
                    .unwrap()
                    .contains("PRIVATE_SENTINEL")
            );
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
