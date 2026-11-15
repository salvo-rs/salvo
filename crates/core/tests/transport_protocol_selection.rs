//! Transport capabilities and TLS negotiation determine the reachable HTTP parser.
#![cfg(all(
    feature = "server",
    feature = "server-handle",
    feature = "http1",
    feature = "http2"
))]

use std::io;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::Empty;
use hyper_util::rt::{TokioExecutor, TokioIo};
use salvo_core::conn::Acceptor;
use salvo_core::prelude::*;

#[handler]
async fn ok() -> &'static str {
    "ok"
}

async fn request<S>(stream: S, h2: bool) -> io::Result<hyper::Response<hyper::body::Incoming>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    tokio::time::timeout(Duration::from_secs(3), async move {
        let message = hyper::Request::get("https://testserver.com/")
            .body(Empty::<Bytes>::new())
            .unwrap();
        if h2 {
            let (mut sender, connection) =
                hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(stream))
                    .await
                    .map_err(io::Error::other)?;
            let task = tokio::spawn(connection);
            let result = sender.send_request(message).await.map_err(io::Error::other);
            task.abort();
            result
        } else {
            let (mut sender, connection) =
                hyper::client::conn::http1::handshake(TokioIo::new(stream))
                    .await
                    .map_err(io::Error::other)?;
            let task = tokio::spawn(connection);
            let result = sender.send_request(message).await.map_err(io::Error::other);
            task.abort();
            result
        }
    })
    .await
    .map_err(io::Error::other)?
}

#[tokio::test]
async fn cleartext_http2_follows_the_feature_and_advertised_capabilities() {
    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    assert_eq!(
        acceptor.holdings()[0]
            .http_versions
            .contains(&salvo_core::http::Version::HTTP_2),
        cfg!(feature = "http2-cleartext")
    );
    let address = acceptor.local_addr().unwrap();
    let server = Server::new(acceptor);
    let handle = server.handle();
    let task = tokio::spawn(server.serve(Router::new().get(ok)));
    let response = request(tokio::net::TcpStream::connect(address).await.unwrap(), true).await;
    if cfg!(feature = "http2-cleartext") {
        assert_eq!(response.unwrap().status(), StatusCode::OK);
    } else {
        assert!(response.is_err());
    }
    assert_eq!(
        request(
            tokio::net::TcpStream::connect(address).await.unwrap(),
            false
        )
        .await
        .unwrap()
        .status(),
        StatusCode::OK
    );
    handle.stop_forceful();
    task.await.unwrap();
}

#[cfg(feature = "rustls")]
mod tls {
    use std::sync::Arc;

    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, ServerName};
    use salvo_core::conn::rustls::{Keycert, RustlsConfig, default_crypto_provider};

    use super::*;

    fn identity() -> Keycert {
        Keycert::new()
            .cert(include_bytes!("../certs/cert.pem").as_slice())
            .key(include_bytes!("../certs/key.pem").as_slice())
    }
    fn client(protocols: Vec<Vec<u8>>) -> tokio_rustls::TlsConnector {
        let mut roots = rustls::RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(include_bytes!("../certs/chain.pem")) {
            roots.add(cert.unwrap()).unwrap();
        }
        let mut config = rustls::ClientConfig::builder_with_provider(default_crypto_provider())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        config.alpn_protocols = protocols;
        tokio_rustls::TlsConnector::from(Arc::new(config))
    }

    #[tokio::test]
    async fn alpn_cannot_be_changed_by_sending_another_http_preface() {
        for (protocol, h2) in [(b"http/1.1".as_slice(), false), (b"h2".as_slice(), true)] {
            let acceptor = TcpListener::new("127.0.0.1:0")
                .rustls(RustlsConfig::new(identity()).alpn_protocols(vec![protocol.to_vec()]))
                .bind()
                .await;
            let address = acceptor.holdings()[0]
                .local_addr
                .clone()
                .into_std()
                .unwrap();
            let server = Server::new(acceptor);
            let handle = server.handle();
            let task = tokio::spawn(server.serve(Router::new().get(ok)));
            for compatible in [false, true] {
                let stream = client(vec![protocol.to_vec()])
                    .connect(
                        ServerName::try_from("testserver.com").unwrap(),
                        tokio::net::TcpStream::connect(address).await.unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(stream.get_ref().1.alpn_protocol(), Some(protocol));
                let response = request(stream, if compatible { h2 } else { !h2 }).await;
                if compatible {
                    assert_eq!(response.unwrap().status(), StatusCode::OK);
                } else {
                    assert!(response.is_err());
                }
            }
            handle.stop_forceful();
            task.await.unwrap();
        }
    }

    #[tokio::test]
    async fn configured_sni_domains_are_case_insensitive() {
        let acceptor = TcpListener::new("127.0.0.1:0")
            .rustls(RustlsConfig::new(None).keycert("TESTSERVER.COM", identity()))
            .bind()
            .await;
        let address = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let server = Server::new(acceptor);
        let handle = server.handle();
        let task = tokio::spawn(server.serve(Router::new().get(ok)));
        let stream = client(vec![b"http/1.1".to_vec()])
            .connect(
                ServerName::try_from("testserver.com").unwrap(),
                tokio::net::TcpStream::connect(address).await.unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            request(stream, false).await.unwrap().status(),
            StatusCode::OK
        );
        handle.stop_forceful();
        task.await.unwrap();
    }

    #[cfg(feature = "quinn")]
    #[tokio::test]
    async fn quic_uses_the_bound_port_and_only_advertises_supported_http3() {
        let config: rustls::ServerConfig = RustlsConfig::new(identity()).try_into().unwrap();
        assert!(
            !config
                .alpn_protocols
                .iter()
                .any(|protocol| protocol == b"h3")
        );
        let acceptor = TcpListener::new("127.0.0.1:0")
            .join(QuinnListener::new(
                RustlsConfig::new(identity()),
                "127.0.0.1:0",
            ))
            .bind()
            .await;
        let tcp = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let udp = acceptor.holdings()[1]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        assert_ne!(udp.port(), 0);
        let server = Server::new(acceptor);
        let handle = server.handle();
        let task = tokio::spawn(server.serve(Router::new().get(ok)));
        let response = request(tokio::net::TcpStream::connect(tcp).await.unwrap(), false)
            .await
            .unwrap();
        assert_eq!(
            response.headers()["alt-svc"],
            format!("h3=\":{}\"; ma=2592000", udp.port())
        );
        handle.stop_forceful();
        task.await.unwrap();
    }
}
