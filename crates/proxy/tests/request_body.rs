//! Verify body replay and framing over a real upstream connection for each client.

#![cfg(any(feature = "hyper-client", feature = "reqwest-client"))]

use std::time::Duration;

use salvo_core::Service;
use salvo_core::conn::{Acceptor, Listener};
use salvo_core::http::{HeaderValue, ParseError, ReqBody, StatusCode};
use salvo_core::prelude::{Request, Response, Router, Server, TcpListener, handler};
use salvo_core::test::{ResponseExt, TestClient};
use salvo_csrf::{CookieStore, Csrf, CsrfDepotExt, FormFinder, HeaderFinder, HmacCipher};
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
async fn read_form(req: &mut Request) {
    assert_eq!(req.form::<String>("field").await.as_deref(), Some("value"));
}

#[handler]
async fn consume_multipart(req: &mut Request) {
    req.form_data().await.unwrap();
    assert!(req.payload().await.is_err());
}

#[handler]
async fn csrf_token(depot: &mut salvo_core::Depot) -> String {
    depot.csrf_token().unwrap().to_owned()
}

#[handler]
async fn checked_form(req: &mut Request) -> Result<String, ParseError> {
    req.try_form::<String>("amount").await
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

        let form_service = Service::new(
            Router::new()
                .hoop(read_form)
                .goal(Proxy::new(target.clone(), client.clone())),
        );
        let form = "field=value&encoded=%2B&empty=&field=second";
        let mut response = TestClient::post("http://localhost/")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .add_header("content-length", "999", true)
            .body(form)
            .send(&form_service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(
            response.headers()["x-received-length"],
            form.len().to_string()
        );
        assert_eq!(response.take_string().await.unwrap(), form);

        let csrf = Csrf::new(
            HmacCipher::new(*b"01234567012345670123456701234567"),
            CookieStore::new(),
            FormFinder::new("csrf_token"),
        )
        .add_finder(HeaderFinder::new("x-csrf-token"));
        let csrf_service = Service::new(
            Router::new()
                .hoop(csrf)
                .push(Router::with_path("token").get(csrf_token))
                .push(Router::with_path("submit").post(Proxy::new(target.clone(), client.clone())))
                .push(Router::with_path("checked").post(checked_form)),
        );
        let mut issued = TestClient::get("http://localhost/token")
            .send(&csrf_service)
            .await;
        let token = issued.take_string().await.unwrap();
        let cookie = issued.cookie("salvo.csrf").unwrap();
        let cookie = format!("{}={}", cookie.name(), cookie.value());
        let encoded_token =
            percent_encoding::utf8_percent_encode(&token, percent_encoding::NON_ALPHANUMERIC);
        let form = format!("csrf_token={encoded_token}&amount=100");
        let mut response = TestClient::post("http://localhost/submit")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .add_header("cookie", &cookie, true)
            .body(form.clone())
            .send(&csrf_service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        assert_eq!(
            response.headers()["x-received-length"],
            form.len().to_string()
        );
        assert_eq!(response.take_string().await.unwrap(), form);

        // FormFinder suppresses its parse error, then a valid header token lets
        // the request continue. The handler must still see the original 413.
        let oversized = format!("amount=100&note={}", "x".repeat(70 * 1024));
        let response = TestClient::post("http://localhost/checked")
            .add_header("content-type", "application/x-www-form-urlencoded", true)
            .add_header("cookie", &cookie, true)
            .add_header("x-csrf-token", &token, true)
            .body(oversized)
            .send(&csrf_service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::PAYLOAD_TOO_LARGE));

        let consumed = Service::new(
            Router::new()
                .hoop(consume_multipart)
                .goal(Proxy::new(target, client)),
        );
        let response = TestClient::post("http://localhost/")
            .add_header("content-type", "multipart/form-data; boundary=b", true)
            .body("--b\r\nContent-Disposition: form-data; name=\"field\"\r\n\r\nvalue\r\n--b--\r\n")
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
