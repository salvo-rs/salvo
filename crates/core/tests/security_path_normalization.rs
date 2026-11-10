//! Regression coverage for path normalization at the service boundary.
#![cfg(feature = "test")]

use salvo_core::catcher::Catcher;
use salvo_core::http::Method;
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};

fn request(method: Method, target: &str) -> Request {
    let mut req = TestClient::get("http://localhost/").build();
    // Preserve the request target: URL builders normalize dot segments.
    *req.uri_mut() = format!("http://localhost{target}")
        .parse()
        .expect("valid test URI");
    *req.method_mut() = method;
    req
}

#[handler]
async fn unreachable_route() {
    panic!("invalid path reached a route handler");
}

#[tokio::test]
async fn invalid_paths_keep_service_middleware_and_custom_catchers() {
    #[handler]
    async fn observe(
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));
        assert!(req.params().is_empty());
        assert_eq!(req.uri().query(), Some("next=%2fpublic%5cfile"));
        depot.insert("observed", true);
        ctrl.call_next(req, depot, res).await;
        res.add_header("x-observed", "yes", true)
            .expect("valid response header");
    }
    #[handler]
    async fn catch(depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        assert!(
            *depot
                .get::<bool>("observed")
                .expect("middleware observed request")
        );
        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));
        res.render("custom bad path");
        ctrl.skip_rest();
    }
    let service = Service::new(
        Router::with_path("{**rest}")
            .filter_fn(|_, _| panic!("invalid path reached a routing filter"))
            .goal(unreachable_route),
    )
    .hoop(observe)
    .catcher(Catcher::default().hoop(catch));

    for path in ["/%2fprivate", "/%5cprivate", "/%00/private"] {
        let mut res = service
            .handle(request(
                Method::GET,
                &format!("{path}?next=%2fpublic%5cfile"),
            ))
            .await;
        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));
        assert_eq!(res.headers()["x-observed"], "yes");
        assert!(!res.headers().contains_key("allow"));
        assert_eq!(
            res.take_string().await.expect("read custom error body"),
            "custom bad path"
        );
    }
}

#[tokio::test]
async fn normalized_routes_preserve_params_query_and_original_uri() {
    #[handler]
    async fn capture(req: &mut Request) -> String {
        assert_eq!(req.param::<String>("id").as_deref(), Some("a b"));
        assert_eq!(req.params().tail(), Some("v1.2/%2f.txt/"));
        assert_eq!(req.uri().query(), Some("next=%2F%5C%00"));
        #[cfg(feature = "matched-path")]
        assert_eq!(req.matched_path(), "users/{id}/{**rest}");
        req.uri().path().to_owned()
    }
    let service = Service::new(Router::with_path("users/{id}/{**rest}").get(capture));
    let path = "/public/%2e%2e/users/a%20b/v1%2e2/%252f.txt/";
    let mut res = service
        .handle(request(Method::GET, &format!("{path}?next=%2F%5C%00")))
        .await;
    assert_eq!(res.status_code, Some(StatusCode::OK));
    assert_eq!(res.take_string().await.expect("read original URI"), path);
}

#[tokio::test]
async fn normalized_paths_keep_method_discovery_and_unknown_paths() {
    #[handler]
    async fn ok() -> &'static str {
        "ok"
    }
    let service = Service::new(Router::with_path("items/{id}").get(ok).post(ok));
    for path in ["/items/42", "/other/../items/42", "/other/%2e%2E/items/42"] {
        let res = service.handle(request(Method::DELETE, path)).await;
        assert_eq!(
            res.status_code,
            Some(StatusCode::METHOD_NOT_ALLOWED),
            "{path}"
        );
        assert_eq!(res.headers()["allow"], "GET, POST");
    }
    let res = service
        .handle(request(Method::DELETE, "/other/../missing"))
        .await;
    assert_eq!(res.status_code, Some(StatusCode::NOT_FOUND));
    assert!(!res.headers().contains_key("allow"));
    for method in [Method::GET, Method::HEAD, Method::DELETE, Method::OPTIONS] {
        let mut res = service.handle(request(method.clone(), "/items%2f42")).await;
        assert_eq!(res.status_code, Some(StatusCode::BAD_REQUEST));
        assert!(!res.headers().contains_key("allow"));
        if method == Method::HEAD {
            assert!(res.body.is_none());
        } else {
            assert!(
                res.take_string()
                    .await
                    .expect("read error body")
                    .contains("invalid request path")
            );
        }
    }
}

#[tokio::test]
async fn normalization_keeps_root_and_single_decode_semantics() {
    #[handler]
    async fn root() -> &'static str {
        "root"
    }
    #[handler]
    async fn capture(req: &mut Request) -> String {
        req.param::<String>("id").expect("matched path parameter")
    }
    let service = Service::new(
        Router::new()
            .get(root)
            .push(Router::with_path("{id}").get(capture)),
    );
    for path in ["/", "/.", "/%2e", "/a/..", "/a/%2e%2e/", "/../../"] {
        let mut res = service.handle(request(Method::GET, path)).await;
        assert_eq!(res.status_code, Some(StatusCode::OK), "{path}");
        assert_eq!(
            res.take_string().await.expect("read root body"),
            "root",
            "{path}"
        );
    }
    for (path, expected) in [
        ("/a+b", "a+b"),
        ("/%252f", "%2f"),
        ("/%2500", "%00"),
        ("/%252e%252e", "%2e%2e"),
        ("/v1%2e2", "v1.2"),
        ("/%E7%94%A8%E6%88%B7", "\u{7528}\u{6237}"),
        ("/hello%20world", "hello world"),
        ("/CONSOLE", "CONSOLE"),
    ] {
        let mut res = service.handle(request(Method::GET, path)).await;
        assert_eq!(res.status_code, Some(StatusCode::OK), "{path}");
        assert_eq!(
            res.take_string().await.expect("read captured parameter"),
            expected,
            "{path}"
        );
    }
}
