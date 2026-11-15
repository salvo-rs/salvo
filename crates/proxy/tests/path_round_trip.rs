//! Routed paths must retain their percent signs over real upstream connections.
#![cfg(any(feature = "hyper-client", feature = "reqwest-client"))]

use std::net::SocketAddr;
use std::time::Duration;

use salvo_core::conn::{Acceptor, Listener};
use salvo_core::prelude::*;
use salvo_proxy::{Client, Proxy};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Task(tokio::task::JoinHandle<()>);
impl Drop for Task {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn spawn_server(router: Router) -> (SocketAddr, Task) {
    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    let address = acceptor.holdings()[0]
        .local_addr
        .clone()
        .into_std()
        .unwrap();
    let task = Task(tokio::spawn(async move {
        Server::new(acceptor).serve(router).await;
    }));
    (address, task)
}

#[handler]
async fn deny(res: &mut Response, ctrl: &mut FlowCtrl) {
    res.status_code(StatusCode::FORBIDDEN);
    ctrl.skip_rest();
}

#[handler]
async fn private() -> &'static str {
    "private report"
}

#[handler]
async fn public(req: &mut Request) -> String {
    format!("public:{}", req.uri().path())
}

async fn request(address: SocketAddr, path: &str, status: u16, body: Option<&str>) {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        stream
            .write_all(
                format!("GET {path} HTTP/1.1\r\nHost: proxy.test\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(
            response.starts_with(&format!("HTTP/1.1 {status} ")),
            "{path}: {response}"
        );
        if let Some(body) = body {
            assert!(response.ends_with(body), "{path}: {response}");
        }
        assert!(!response.contains("private report"), "{path}: {response}");
    })
    .await
    .expect("proxied path request must complete");
}

async fn path_matrix<C: Client + Clone>(client: C) {
    let (upstream, _upstream) = spawn_server(
        Router::new()
            .push(Router::with_path("report_note/{**rest}").goal(private))
            .push(Router::with_path("{**rest}").goal(public)),
    )
    .await;
    let target = format!("http://{upstream}");
    let (strict, _strict) = spawn_server(
        Router::new()
            .push(Router::with_path("report_note/{**rest}").goal(deny))
            .push(Router::with_path("{**rest}").goal(Proxy::new(target.clone(), client.clone()))),
    )
    .await;
    for path in ["/report_note/secret.txt", "/%72eport_note/secret.txt"] {
        request(strict, path, 403, None).await;
    }
    for path in [
        "/%2572eport_note/secret.txt",
        "/report%255fnote/secret.txt",
        "/files/a%2541.txt",
        "/files/a%25%34%31.txt",
        "/api/x%2500y",
        "/api/x%250d%250ay",
        "/%252e%252e/admin",
    ] {
        request(strict, path, 400, None).await;
    }
    for path in [
        "/files/50%25off.txt",
        "/files/100%25",
        "/files/100%254",
        "/files/hello%20world.txt",
        "/files/a%3Fb%23c",
        "/files/%E7%94%A8%E6%88%B7",
    ] {
        request(strict, path, 200, Some(&format!("public:{path}"))).await;
    }

    let (lenient, _lenient) = spawn_server(
        Router::with_path("{**rest}")
            .goal(Proxy::new(target.clone(), client.clone()).strict_path_normalization(false)),
    )
    .await;
    for path in [
        "/files/a%2541.txt",
        "/%2572eport_note/secret.txt",
        "/%252e%252e/secret.txt",
        "/api/x%2500y",
        "/api/x%250d%250ay",
    ] {
        request(lenient, path, 200, Some(&format!("public:{path}"))).await;
    }

    // Custom getters already return wire escapes. They must keep their old
    // encoding contract instead of having their percent signs escaped again.
    let (custom, _custom) = spawn_server(
        Router::with_path("{**rest}").goal(
            Proxy::new(target.clone(), client.clone())
                .url_path_getter(|req, _| Some(req.uri().path().to_owned())),
        ),
    )
    .await;
    for path in ["/files/50%25off.txt", "/files/a%20b%3Fc%23d"] {
        request(custom, path, 200, Some(&format!("public:{path}"))).await;
    }
    request(custom, "/files/a%41.txt", 400, None).await;
    let (custom_lenient, _custom_lenient) = spawn_server(
        Router::with_path("{**rest}").goal(
            Proxy::new(target, client)
                .strict_path_normalization(false)
                .url_path_getter(|req, _| Some(req.uri().path().to_owned())),
        ),
    )
    .await;
    request(
        custom_lenient,
        "/files/a%41.txt",
        200,
        Some("public:/files/a%41.txt"),
    )
    .await;
}

#[cfg(feature = "hyper-client")]
#[tokio::test]
async fn hyper_preserves_routed_percent_signs_and_blocks_double_decode() {
    path_matrix(salvo_proxy::HyperClient::default()).await;
}

#[cfg(feature = "reqwest-client")]
#[tokio::test]
async fn reqwest_preserves_routed_percent_signs_and_blocks_double_decode() {
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    path_matrix(salvo_proxy::ReqwestClient::new(client)).await;
}
