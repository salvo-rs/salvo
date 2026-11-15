//! Verify body replay and framing over a real upstream connection for each client.

#![cfg(any(feature = "hyper-client", feature = "reqwest-client"))]

use std::time::Duration;

use salvo_core::Service;
use salvo_core::conn::{Acceptor, Listener};
use salvo_core::http::{HeaderValue, ReqBody, StatusCode};
use salvo_core::prelude::{Request, Response, Router, Server, TcpListener, handler};
use salvo_core::test::{ResponseExt, TestClient};
use salvo_proxy::{Client, Proxy};

struct Task(tokio::task::JoinHandle<()>);
impl Drop for Task {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[handler]
async fn echo(req: &mut Request, res: &mut Response) {
    let length = req
        .headers()
        .get("content-length")
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("unknown"));
    res.headers_mut().insert("x-received-length", length);
    res.headers_mut().insert(
        "x-received-path",
        HeaderValue::from_str(req.uri().path()).unwrap(),
    );
    res.write_body(req.payload().await.unwrap().clone())
        .unwrap();
}

#[handler]
async fn inspect_payload(req: &mut Request) {
    req.payload().await.unwrap();
}

#[handler]
async fn replace_payload(req: &mut Request) {
    req.payload().await.unwrap();
    req.replace_body(ReqBody::from("replacement"));
}

#[handler]
async fn consume_form(req: &mut Request) {
    req.form_data().await.unwrap();
    assert!(req.payload().await.is_err());
}

async fn round_trip<C: Client + Clone>(client: C) {
    tokio::time::timeout(Duration::from_secs(10), async move {
        let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
        let address = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let _upstream = Task(tokio::spawn(async move {
            Server::new(acceptor)
                .serve(Router::with_path("{**rest}").goal(echo))
                .await;
        }));
        let target = format!("http://{address}");
        let service = Service::new(
            Router::with_path("allowed")
                .hoop(inspect_payload)
                .goal(Proxy::new(target.clone(), client.clone())),
        );

        // Include request-looking bytes and then send another request using
        // the same upstream pool. Each must remain one correctly framed body.
        for payload in [
            "GET /private HTTP/1.1\r\nHost: upstream\r\n\r\n",
            "next request",
        ] {
            let mut response = TestClient::post("http://localhost/allowed")
                .add_header("content-length", "999", true)
                .body(payload)
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::OK));
            assert_eq!(
                response.headers()["x-received-length"],
                payload.len().to_string()
            );
            assert_eq!(response.headers()["x-received-path"], "/");
            assert_eq!(response.take_string().await.unwrap(), payload);
        }
        let denied = TestClient::get("http://localhost/private")
            .send(&service)
            .await;
        assert_eq!(denied.status_code, Some(StatusCode::NOT_FOUND));

        let replaced = Service::new(
            Router::new()
                .hoop(replace_payload)
                .goal(Proxy::new(target.clone(), client.clone())),
        );
        let mut response = TestClient::post("http://localhost/")
            .add_header("content-length", "999", true)
            .body("old")
            .send(&replaced)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(response.headers()["x-received-length"], "11");
        assert_eq!(response.take_string().await.unwrap(), "replacement");

        let consumed = Service::new(
            Router::new()
                .hoop(consume_form)
                .goal(Proxy::new(target, client)),
        );
        let response = TestClient::post("http://localhost/")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .body("field=value")
            .send(&consumed)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::BAD_REQUEST));
    })
    .await
    .expect("body forwarding must complete without a framing deadlock");
}

#[cfg(feature = "hyper-client")]
#[tokio::test]
async fn hyper_client_replays_and_frames_the_forwarded_body() {
    round_trip(salvo_proxy::HyperClient::default()).await;
}

#[cfg(feature = "reqwest-client")]
#[tokio::test]
async fn reqwest_client_replays_and_frames_the_forwarded_body() {
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    round_trip(salvo_proxy::ReqwestClient::new(client)).await;
}
