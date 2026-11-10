//! Host isolation must not depend on the HTTP/1 URI-normalization feature.
#![allow(missing_docs, clippy::unwrap_used)]

use std::time::Duration;

use salvo_cache::{Cache, CacheIssuer, MokaStore, RequestIssuer};
use salvo_core::conn::Acceptor;
use salvo_core::prelude::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn origin_form_keys_use_a_single_valid_host_or_decline_caching() {
    let mut req = Request::new();
    *req.uri_mut() = "/shared".parse().unwrap();
    let issuer = RequestIssuer::new();
    assert!(issuer.issue(&mut req, &Depot::new()).await.is_none());
    req.add_header("host", "one.example", true).unwrap();
    let one = issuer.issue(&mut req, &Depot::new()).await.unwrap();
    req.add_header("host", "two.example", true).unwrap();
    let two = issuer.issue(&mut req, &Depot::new()).await.unwrap();
    assert_ne!(one, two);
    req.add_header("host", "three.example", false).unwrap();
    assert!(issuer.issue(&mut req, &Depot::new()).await.is_none());
    for host in ["invalid host", "user@example.com", "", "example.com/path"] {
        req.add_header("host", host, true).unwrap();
        assert!(
            issuer.issue(&mut req, &Depot::new()).await.is_none(),
            "{host}"
        );
    }
    assert!(
        RequestIssuer::new()
            .use_authority(false)
            .issue(&mut req, &Depot::new())
            .await
            .is_some()
    );
}

#[handler]
async fn host_page(req: &mut Request) -> String {
    eprintln!(
        "origin-form URI has authority: {}",
        req.uri().authority().is_some()
    );
    req.header::<String>("host").unwrap()
}

#[tokio::test]
async fn real_origin_form_requests_do_not_share_representations_across_hosts() {
    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    let addr = acceptor.holdings()[0]
        .local_addr
        .clone()
        .into_std()
        .unwrap();
    let server = Server::new(acceptor);
    let handle = server.handle();
    let router = Router::with_path("shared")
        .hoop(Cache::new(MokaStore::new(10), RequestIssuer::new()))
        .get(host_page);
    let task = tokio::spawn(server.try_serve(router));
    for host in ["one.example", "two.example", "one.example", "two.example"] {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                format!("GET /shared HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        let response = String::from_utf8(bytes).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.ends_with(host), "{response}");
    }
    handle.stop_forceful();
    tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
