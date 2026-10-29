//! A proxy must accept the upstream's selection from the client's Upgrade list.

#![cfg(feature = "hyper-client")]

use std::time::Duration;

use salvo_core::conn::{Acceptor, Listener};
use salvo_core::prelude::{Router, Server, TcpListener};
use salvo_proxy::{Client, HyperClient, Proxy};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Task(tokio::task::JoinHandle<()>);
impl Drop for Task {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn read_head(stream: &mut tokio::net::TcpStream) -> String {
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        head.push(stream.read_u8().await.unwrap());
        assert!(head.len() < 8192);
    }
    String::from_utf8(head).unwrap()
}

async fn round_trip<C: Client>(client: C, offered: &[&str], selected: &[&str], accepted: bool) {
    let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();
    let expected_offer = offered.join(", ");
    let expected_selection = selected.join(", ");
    let selected = selected
        .iter()
        .map(|value| format!("Upgrade: {value}\r\n"))
        .collect::<String>();
    let mut upstream = Task(tokio::spawn(async move {
        let (mut stream, _) = upstream_listener.accept().await.unwrap();
        let head = read_head(&mut stream).await;
        assert!(head.to_ascii_lowercase().contains("connection: upgrade"));
        assert!(
            head.to_ascii_lowercase().contains(&format!(
                "upgrade: {}\r\n",
                expected_offer.to_ascii_lowercase()
            )),
            "{head}"
        );
        stream
            .write_all(
                format!(
                    "HTTP/1.1 101 Switching Protocols\r\nConnection: upgrade\r\n{selected}\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        if accepted {
            let mut data = [0; 4];
            stream.read_exact(&mut data).await.unwrap();
            assert_eq!(&data, b"ping");
            stream.write_all(b"pong").await.unwrap();
        }
    }));
    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    let proxy_addr = acceptor.holdings()[0]
        .local_addr
        .clone()
        .into_std()
        .unwrap();
    let proxy = Task(tokio::spawn(async move {
        Server::new(acceptor)
            .serve(Router::new().goal(Proxy::new(format!("http://{upstream_addr}"), client)))
            .await;
    }));
    let mut stream = tokio::net::TcpStream::connect(proxy_addr).await.unwrap();
    let mut request = format!("GET / HTTP/1.1\r\nHost: {proxy_addr}\r\nConnection: upgrade\r\n");
    for protocol in offered {
        request.push_str(&format!("Upgrade: {protocol}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let head = read_head(&mut stream).await;
    if accepted {
        assert!(head.starts_with("HTTP/1.1 101 "), "{head}");
        assert!(
            head.to_ascii_lowercase().contains(&format!(
                "upgrade: {}\r\n",
                expected_selection.to_ascii_lowercase()
            )),
            "{head}"
        );
        stream.write_all(b"ping").await.unwrap();
        let mut data = [0; 4];
        stream.read_exact(&mut data).await.unwrap();
        assert_eq!(&data, b"pong");
    } else {
        assert!(head.starts_with("HTTP/1.1 500 "), "{head}");
    }
    drop(stream);
    tokio::time::timeout(Duration::from_secs(5), &mut upstream.0)
        .await
        .unwrap()
        .unwrap();
    drop(proxy);
}

#[tokio::test]
async fn hyper_accepts_an_offered_protocol_from_a_list() {
    tokio::time::timeout(Duration::from_secs(15), async {
        for offers in [vec!["other, Chat/1"], vec!["other", "Chat/1"]] {
            round_trip(HyperClient::default(), &offers, &["cHaT/1"], true).await;
        }
    })
    .await
    .unwrap();
}

#[cfg(feature = "reqwest-client")]
#[tokio::test]
async fn reqwest_accepts_an_offered_protocol_from_a_list() {
    tokio::time::timeout(Duration::from_secs(15), async {
        for offers in [vec!["other, Chat/1"], vec!["other", "Chat/1"]] {
            round_trip(
                salvo_proxy::ReqwestClient::default(),
                &offers,
                &["cHaT/1"],
                true,
            )
            .await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn hyper_preserves_layered_selections_across_response_fields() {
    tokio::time::timeout(Duration::from_secs(15), async {
        round_trip(
            HyperClient::default(),
            &["other, Chat/1, Extra"],
            &["cHaT/1", "extra"],
            true,
        )
        .await;
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn hyper_rejects_an_unoffered_protocol_or_version() {
    tokio::time::timeout(Duration::from_secs(15), async {
        for selected in ["other", "Chat/2", "Chat/1, other"] {
            round_trip(HyperClient::default(), &["Chat/1"], &[selected], false).await;
        }
    })
    .await
    .unwrap();
}
