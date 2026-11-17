#![allow(missing_docs, clippy::unwrap_used)]
#![cfg(feature = "test")]

use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};

#[handler]
async fn original_uri(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
    ctrl: &mut FlowCtrl,
) {
    let uri = req.uri().clone();
    let target = req.uri().path_and_query().unwrap().as_str().to_owned();
    depot.insert("signed_target", target.clone());
    ctrl.call_next(req, depot, res).await;
    assert_eq!(req.uri(), &uri);
    assert_eq!(req.uri().path_and_query().unwrap().as_str(), target);
}

#[handler]
async fn profile(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    res.render(Json(serde_json::json!({
        "id": req.param::<String>("user_id").unwrap(),
        "target": req.uri().path_and_query().unwrap().as_str(),
        "signed_target": depot.get::<String>("signed_target").unwrap(),
    })));
}

fn request(target: &str) -> Request {
    let mut req = TestClient::get("http://localhost/").build();
    *req.uri_mut() = target.parse().unwrap();
    req
}

#[tokio::test]
async fn matrix_ids_are_one_parameter_and_original_signed_target_is_unchanged() {
    let service = Service::new(
        Router::with_path("_matrix/client/v3/profile/{user_id}")
            .hoop(original_uri)
            .get(profile)
            .push(Router::with_path("avatar_url").get(profile)),
    );
    for (encoded, decoded) in [
        ("@alice:example.org", "@alice:example.org"),
        ("@alice%3Aexample.org", "@alice:example.org"),
        ("@a%2Fb:example.org", "@a/b:example.org"),
        ("@a%2fb%3aexample.org", "@a/b:example.org"),
        ("%40a%2Fb%3Aexample.org%3A8448", "@a/b:example.org:8448"),
        ("@a+b=._-:example.org", "@a+b=._-:example.org"),
        ("@a%2Bb:example.org", "@a+b:example.org"),
        (
            "@alice:%5B2001%3Adb8%3A%3A1%5D%3A8448",
            "@alice:[2001:db8::1]:8448",
        ),
        // This is routing data, not a valid MXID. Business validation may reject
        // it, but the router must never decode the percent escape twice.
        ("@a%252Fb:example.org", "@a%2Fb:example.org"),
    ] {
        for suffix in ["", "/avatar_url"] {
            let target =
                format!("/_matrix/client/v3/profile/{encoded}{suffix}?b=%2f&a=1&a=2&plus=+");
            let mut response = service.handle(request(&target)).await;
            assert_eq!(response.status_code, Some(StatusCode::OK), "{target}");
            let value: serde_json::Value = response.take_json().await.unwrap();
            assert_eq!(value["id"], decoded, "{target}");
            assert_eq!(value["target"], target);
            assert_eq!(value["signed_target"], target);
        }
    }
    let response = service
        .handle(request("/_matrix/client/v3/profile/@a/b:example.org"))
        .await;
    assert_eq!(response.status_code, Some(StatusCode::NOT_FOUND));
    let long_id = format!("@{}/{}:example.org", "a".repeat(120), "b".repeat(121));
    assert_eq!(long_id.len(), 255);
    let target = format!("/_matrix/client/v3/profile/{}", long_id.replace('/', "%2F"));
    let mut response = service.handle(request(&target)).await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    let value: serde_json::Value = response.take_json().await.unwrap();
    assert_eq!(value["id"], long_id);
}

#[handler]
async fn tail(req: &mut Request, res: &mut Response) {
    res.render(Json(serde_json::json!({
        "decoded": req.params().tail_segments().unwrap().collect::<Vec<_>>(),
        "encoded": req.params().tail_raw_segments().unwrap().collect::<Vec<_>>(),
        "tail": req.params().tail().unwrap(),
        "target": req.uri().path_and_query().unwrap().as_str(),
    })));
}

#[tokio::test]
async fn wildcard_forms_keep_boundaries_encoding_and_partial_utf8_offsets() {
    for pattern in [
        "{**rest}",
        "{*?rest}",
        #[cfg(feature = "routing-regex")]
        "{**rest|.*}",
        #[cfg(feature = "routing-regex")]
        "{*?rest|.*}",
        #[cfg(feature = "routing-regex")]
        "prefix{**rest}",
    ] {
        let prefix = if pattern.starts_with("prefix") {
            "pr%65fix"
        } else {
            ""
        };
        let service = Service::new(Router::with_path(pattern).goal(tail));
        let target = format!("/{prefix}@a%2fb%3Aexample.org/");
        let mut response = service.handle(request(&target)).await;
        assert_eq!(response.status_code, Some(StatusCode::OK), "{pattern}");
        let value: serde_json::Value = response.take_json().await.unwrap();
        assert_eq!(
            value["decoded"],
            serde_json::json!(["@a/b:example.org", ""])
        );
        assert_eq!(
            value["encoded"],
            serde_json::json!(["@a%2fb%3Aexample.org", ""])
        );
        assert_eq!(value["target"], target);
    }
    #[cfg(feature = "routing-regex")]
    {
        let service = Service::new(Router::with_path("é{**rest}").goal(tail));
        let mut response = service.handle(request("/%C3%A9@a%2Fb:example.org")).await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        let value: serde_json::Value = response.take_json().await.unwrap();
        assert_eq!(value["encoded"], serde_json::json!(["@a%2Fb:example.org"]));
    }
}

#[tokio::test]
async fn failed_wildcard_branch_rolls_back_segment_metadata() {
    let service = Service::new(
        Router::new()
            .push(
                Router::with_path("{**failed}")
                    .filter_fn(|_, _| false)
                    .goal(tail),
            )
            .push(Router::with_path("prefix/{**rest}").goal(tail)),
    );
    let mut response = service.handle(request("/prefix/a%2Fb/c%255Cd")).await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    let value: serde_json::Value = response.take_json().await.unwrap();
    assert_eq!(value["decoded"], serde_json::json!(["a/b", "c%5Cd"]));
    assert_eq!(value["encoded"], serde_json::json!(["a%2Fb", "c%255Cd"]));
}

#[tokio::test]
async fn wildcard_can_reuse_an_ancestor_parameter_name() {
    let service = Service::new(Router::with_path("{rest}/{other}/{**rest}").goal(tail));
    let mut response = service.handle(request("/ancestor/other/a%2Fb/c")).await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    let value: serde_json::Value = response.take_json().await.unwrap();
    assert_eq!(value["decoded"], serde_json::json!(["a/b", "c"]));
    assert_eq!(value["tail"], "a/b/c");
}

#[tokio::test]
async fn invalid_utf8_nul_and_literal_backslashes_stay_rejected() {
    let service = Service::new(Router::with_path("{**rest}").goal(tail));
    for path in ["/%FF", "/%C3%28", "/%00/..", "/a\\b", "/a%00b"] {
        assert_eq!(
            service.handle(request(path)).await.status_code,
            Some(StatusCode::BAD_REQUEST),
            "{path}"
        );
    }
}

#[cfg(all(feature = "server", feature = "http1"))]
#[tokio::test]
async fn matrix_ids_and_signature_target_survive_real_http() {
    use salvo_core::conn::Acceptor;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    let address = acceptor.holdings()[0]
        .local_addr
        .clone()
        .into_std()
        .unwrap();
    let server = Server::new(acceptor);
    let task = tokio::spawn(
        server.try_serve(
            Router::with_path("_matrix/client/v3/profile/{user_id}")
                .hoop(original_uri)
                .get(profile),
        ),
    );
    for (id, expected) in [
        ("@alice:example.org", "@alice:example.org"),
        ("@a%2fb%3Aexample.org", "@a/b:example.org"),
        ("@a%252Fb:example.org", "@a%2Fb:example.org"),
    ] {
        let target = format!("/_matrix/client/v3/profile/{id}?b=%2f&a=1&a=2&plus=+");
        let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
        client
            .write_all(
                format!("GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let mut response = Vec::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            client.read_to_end(&mut response),
        )
        .await
        .unwrap()
        .unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 "), "{response}");
        assert!(
            response.contains(&format!("\"id\":\"{expected}\"")),
            "{response}"
        );
        assert!(
            response.contains(&format!("\"signed_target\":\"{target}\"")),
            "{response}"
        );
        assert!(
            response.contains(&format!("\"target\":\"{target}\"")),
            "{response}"
        );
    }
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
}
