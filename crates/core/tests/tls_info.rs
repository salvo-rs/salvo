//! Exercise the public Request API through real sockets and TLS handshakes.
#![cfg(all(feature = "server", feature = "server-handle", feature = "http1"))]
#![allow(clippy::unwrap_used)]

use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper_util::rt::TokioIo;
use salvo_core::conn::{Acceptor, TcpListener};
use salvo_core::prelude::*;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;

#[handler]
async fn inspect(req: &mut Request, res: &mut Response) {
    res.render(Json(match req.tls_info() {
        None => Value::Null,
        Some(info) => json!({
            "certificates": info.peer_certificates(),
            "sni": info.server_name(),
            "alpn": info.alpn_protocol(),
            "shared": format!("{info:p}"),
        }),
    }));
}

struct Running {
    handle: salvo_core::server::ServerHandle,
    task: tokio::task::JoinHandle<()>,
}

impl Running {
    fn start<A: Acceptor + 'static>(acceptor: A) -> Self {
        let server = Server::new(acceptor);
        let handle = server.handle();
        let task = tokio::spawn(server.serve(Router::new().get(inspect)));
        Self { handle, task }
    }

    async fn stop(self) {
        self.handle.stop_forceful();
        tokio::time::timeout(Duration::from_secs(5), self.task)
            .await
            .unwrap()
            .unwrap();
    }
}

async fn http1<S>(stream: S) -> hyper::client::conn::http1::SendRequest<Empty<Bytes>>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });
    sender
}

fn request() -> hyper::Request<Empty<Bytes>> {
    hyper::Request::builder()
        .uri("https://localhost/")
        .header("host", "localhost")
        .body(Empty::new())
        .unwrap()
}

async fn decode(response: hyper::Response<hyper::body::Incoming>) -> Value {
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn plain_http_has_no_tls_info_even_with_https_uri_or_forwarded_headers() {
    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    let addr = acceptor.local_addr().unwrap();
    let server = Running::start(acceptor);
    let mut sender = http1(TcpStream::connect(addr).await.unwrap()).await;
    let mut req = request();
    req.headers_mut()
        .insert("x-forwarded-proto", "https".parse().unwrap());
    req.headers_mut()
        .insert("x-forwarded-client-cert", "spoofed".parse().unwrap());
    assert_eq!(
        decode(sender.send_request(req).await.unwrap()).await,
        Value::Null
    );
    server.stop().await;
}

#[test]
fn constructed_request_has_no_tls_info() {
    assert!(Request::new().tls_info().is_none());
}

#[cfg(feature = "rustls")]
mod tls {
    use std::sync::Arc;

    use rcgen::{
        BasicConstraints, Certificate, CertificateParams, CertifiedIssuer, DnType,
        ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose,
    };
    use rustls::pki_types::{PrivatePkcs8KeyDer, ServerName};
    use rustls::{ClientConfig, RootCertStore};
    use salvo_core::conn::rustls::{Keycert, RustlsConfig, default_crypto_provider};
    use tokio_rustls::TlsConnector;
    use tokio_rustls::client::TlsStream;

    use super::*;

    struct Identity {
        certificate: Certificate,
        key: KeyPair,
    }

    struct Pki {
        ca: CertifiedIssuer<'static, KeyPair>,
        intermediate: CertifiedIssuer<'static, KeyPair>,
        server: Identity,
        alice: Identity,
        bob: Identity,
    }

    fn ca_params() -> CertificateParams {
        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        params
    }

    fn identity(
        name: &str,
        usage: ExtendedKeyUsagePurpose,
        issuer: &rcgen::Issuer<'_, KeyPair>,
    ) -> Identity {
        let mut params = CertificateParams::new(vec![name.to_owned()]).unwrap();
        params.distinguished_name.push(DnType::CommonName, name);
        params.extended_key_usages = vec![usage];
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        let key = KeyPair::generate().unwrap();
        let certificate = params.signed_by(&key, issuer).unwrap();
        Identity { certificate, key }
    }

    impl Pki {
        fn new() -> Self {
            let mut params = ca_params();
            params
                .distinguished_name
                .push(DnType::CommonName, "test root");
            let ca = CertifiedIssuer::self_signed(params, KeyPair::generate().unwrap()).unwrap();
            let mut params = ca_params();
            params
                .distinguished_name
                .push(DnType::CommonName, "test intermediate");
            let intermediate =
                CertifiedIssuer::signed_by(params, KeyPair::generate().unwrap(), &ca).unwrap();
            let server = identity("localhost", ExtendedKeyUsagePurpose::ServerAuth, &ca);
            let alice = identity("alice", ExtendedKeyUsagePurpose::ClientAuth, &intermediate);
            let bob = identity("bob", ExtendedKeyUsagePurpose::ClientAuth, &intermediate);
            Self {
                ca,
                intermediate,
                server,
                alice,
                bob,
            }
        }

        fn config(&self) -> RustlsConfig {
            RustlsConfig::new(
                Keycert::new()
                    .cert(self.server.certificate.pem())
                    .key(self.server.key.serialize_pem()),
            )
        }

        fn client(&self, identity: Option<(&Identity, &Self)>, alpn: &[u8]) -> ClientConfig {
            let mut roots = RootCertStore::empty();
            roots.add(self.ca.der().clone()).unwrap();
            let builder = ClientConfig::builder_with_provider(default_crypto_provider())
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(roots);
            let mut client = match identity {
                Some((identity, issuer)) => builder
                    .with_client_auth_cert(
                        vec![
                            identity.certificate.der().clone(),
                            issuer.intermediate.der().clone(),
                        ],
                        PrivatePkcs8KeyDer::from(identity.key.serialize_der()).into(),
                    )
                    .unwrap(),
                None => builder.with_no_client_auth(),
            };
            client.alpn_protocols = vec![alpn.to_vec()];
            client
        }
    }

    async fn connect(
        addr: std::net::SocketAddr,
        client: ClientConfig,
    ) -> Result<TlsStream<TcpStream>, std::io::Error> {
        TlsConnector::from(Arc::new(client))
            .connect(
                ServerName::try_from("localhost").unwrap(),
                TcpStream::connect(addr).await?,
            )
            .await
    }

    fn assert_info(info: &Value, pki: &Pki, identity: Option<&Identity>, alpn: &[u8]) {
        assert_eq!(info["sni"], "localhost");
        assert_eq!(info["alpn"], json!(alpn));
        let expected = identity
            .map(|identity| {
                vec![
                    identity.certificate.der().to_vec(),
                    pki.intermediate.der().to_vec(),
                ]
            })
            .unwrap_or_default();
        assert_eq!(info["certificates"], json!(expected));
    }

    #[test]
    fn optional_and_required_auth_do_not_need_a_process_default_provider() {
        // This integration test binary never installs a provider. Its dev-dependencies
        // deliberately enable both Rustls crypto backends, reproducing the old panic.
        assert!(rustls::crypto::CryptoProvider::get_default().is_none());
        let pki = Pki::new();
        for config in [
            pki.config().client_auth_required(pki.ca.pem()),
            pki.config().client_auth_optional(pki.ca.pem()),
        ] {
            let _: rustls::ServerConfig = config.try_into().unwrap();
        }
        assert!(rustls::crypto::CryptoProvider::get_default().is_none());
    }

    #[tokio::test]
    async fn https_without_client_auth_still_exposes_tls_info() {
        let pki = Pki::new();
        let acceptor = TcpListener::new("127.0.0.1:0")
            .rustls(pki.config())
            .bind()
            .await;
        let addr = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let server = Running::start(acceptor);
        let mut sender = http1(connect(addr, pki.client(None, b"http/1.1")).await.unwrap()).await;
        assert_info(
            &decode(sender.send_request(request()).await.unwrap()).await,
            &pki,
            None,
            b"http/1.1",
        );
        server.stop().await;
    }

    #[tokio::test]
    async fn tls_info_is_available_without_sni_or_alpn() {
        let pki = Pki::new();
        let acceptor = TcpListener::new("127.0.0.1:0")
            .rustls(pki.config())
            .bind()
            .await;
        let addr = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let server = Running::start(acceptor);
        let mut client = pki.client(None, b"http/1.1");
        client.enable_sni = false;
        client.alpn_protocols.clear();
        let mut sender = http1(connect(addr, client).await.unwrap()).await;
        let info = decode(sender.send_request(request()).await.unwrap()).await;
        assert!(info.is_object());
        assert_eq!(info["sni"], Value::Null);
        assert_eq!(info["alpn"], Value::Null);
        assert_eq!(info["certificates"], json!([]));
        server.stop().await;
    }

    #[tokio::test]
    async fn optional_auth_distinguishes_anonymous_tls_from_plain_http() {
        let pki = Pki::new();
        let acceptor = TcpListener::new("127.0.0.1:0")
            .rustls(pki.config().client_auth_optional(pki.ca.pem()))
            .join(TcpListener::new("127.0.0.1:0"))
            .bind()
            .await;
        let tls_addr = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let plain_addr = acceptor.holdings()[1]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let server = Running::start(acceptor);
        let mut anonymous = http1(
            connect(tls_addr, pki.client(None, b"http/1.1"))
                .await
                .unwrap(),
        )
        .await;
        assert_info(
            &decode(anonymous.send_request(request()).await.unwrap()).await,
            &pki,
            None,
            b"http/1.1",
        );
        let mut authenticated = http1(
            connect(tls_addr, pki.client(Some((&pki.alice, &pki)), b"http/1.1"))
                .await
                .unwrap(),
        )
        .await;
        assert_info(
            &decode(authenticated.send_request(request()).await.unwrap()).await,
            &pki,
            Some(&pki.alice),
            b"http/1.1",
        );
        let mut plain = http1(TcpStream::connect(plain_addr).await.unwrap()).await;
        assert_eq!(
            decode(plain.send_request(request()).await.unwrap()).await,
            Value::Null
        );
        server.stop().await;
    }

    #[tokio::test]
    async fn required_auth_shares_metadata_on_keep_alive_and_isolates_connections_when_boxed() {
        let pki = Pki::new();
        let acceptor = TcpListener::new("127.0.0.1:0")
            .rustls(pki.config().client_auth_required(pki.ca.pem()))
            .bind()
            .await;
        let addr = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let acceptor = salvo_core::conn::tcp::DynTcpAcceptors::new(vec![acceptor.into_boxed()]);
        let server = Running::start(acceptor);
        let mut alice = http1(
            connect(addr, pki.client(Some((&pki.alice, &pki)), b"http/1.1"))
                .await
                .unwrap(),
        )
        .await;
        let mut bob = http1(
            connect(addr, pki.client(Some((&pki.bob, &pki)), b"http/1.1"))
                .await
                .unwrap(),
        )
        .await;
        let a1 = decode(alice.send_request(request()).await.unwrap()).await;
        let b = decode(bob.send_request(request()).await.unwrap()).await;
        let a2 = decode(alice.send_request(request()).await.unwrap()).await;
        assert_info(&a1, &pki, Some(&pki.alice), b"http/1.1");
        assert_info(&b, &pki, Some(&pki.bob), b"http/1.1");
        assert_eq!(a1, a2, "requests must borrow the same allocation");
        assert_ne!(a1["shared"], b["shared"]);
        server.stop().await;
    }

    #[tokio::test]
    async fn resumed_tls_connections_retain_the_authenticated_certificate_chain() {
        let pki = Pki::new();
        let acceptor = TcpListener::new("127.0.0.1:0")
            .rustls(pki.config().client_auth_required(pki.ca.pem()))
            .bind()
            .await;
        let addr = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let server = Running::start(acceptor);
        // ClientConfig clones share the session cache. Reading the first response
        // also processes the session tickets sent after the full handshake.
        let client = pki.client(Some((&pki.alice, &pki)), b"http/1.1");
        for expected in [rustls::HandshakeKind::Full, rustls::HandshakeKind::Resumed] {
            let stream = connect(addr, client.clone()).await.unwrap();
            assert_eq!(stream.get_ref().1.handshake_kind(), Some(expected));
            let mut sender = http1(stream).await;
            assert_info(
                &decode(sender.send_request(request()).await.unwrap()).await,
                &pki,
                Some(&pki.alice),
                b"http/1.1",
            );
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn missing_and_untrusted_certificates_are_rejected_before_http() {
        let pki = Pki::new();
        let foreign = Pki::new();
        for optional in [false, true] {
            let config = if optional {
                pki.config().client_auth_optional(pki.ca.pem())
            } else {
                pki.config().client_auth_required(pki.ca.pem())
            };
            let acceptor = TcpListener::new("127.0.0.1:0").rustls(config).bind().await;
            let addr = acceptor.holdings()[0]
                .local_addr
                .clone()
                .into_std()
                .unwrap();
            let server = Running::start(acceptor);
            for identity in [None, Some((&foreign.alice, &foreign))] {
                if optional && identity.is_none() {
                    continue;
                }
                tokio::time::timeout(Duration::from_secs(5), async {
                    // TLS 1.3 clients can finish locally before receiving the server's
                    // rejection. In that case the first HTTP read must fail instead.
                    if let Ok(stream) = connect(addr, pki.client(identity, b"http/1.1")).await {
                        let mut sender = http1(stream).await;
                        assert!(sender.send_request(request()).await.is_err());
                    }
                })
                .await
                .expect("rejected handshake must not hang");
            }
            server.stop().await;
        }
    }

    #[cfg(feature = "http2")]
    #[tokio::test]
    async fn http2_streams_share_authenticated_connection_metadata() {
        let pki = Pki::new();
        let acceptor = TcpListener::new("127.0.0.1:0")
            .rustls(pki.config().client_auth_required(pki.ca.pem()))
            .bind()
            .await;
        let addr = acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap();
        let server = Running::start(acceptor);
        let stream = connect(addr, pki.client(Some((&pki.alice, &pki)), b"h2"))
            .await
            .unwrap();
        let (mut sender, connection) = hyper::client::conn::http2::handshake(
            hyper_util::rt::TokioExecutor::new(),
            TokioIo::new(stream),
        )
        .await
        .unwrap();
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let first = sender.send_request(request());
        let second = sender.send_request(request());
        let (first, second) = tokio::join!(first, second);
        let first = decode(first.unwrap()).await;
        let second = decode(second.unwrap()).await;
        assert_info(&first, &pki, Some(&pki.alice), b"h2");
        assert_eq!(first, second);
        server.stop().await;
    }

    #[cfg(feature = "quinn")]
    #[tokio::test]
    async fn http3_requests_expose_authenticated_tls_info() {
        use bytes::Buf;

        let pki = Pki::new();
        let config = pki
            .config()
            .client_auth_required(pki.ca.pem())
            .alpn_protocols(vec![b"h3".to_vec()]);
        let endpoint = quinn::Endpoint::server(
            config.build_quinn_config().unwrap(),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let addr = endpoint.local_addr().unwrap();
        let acceptor = salvo_core::conn::quinn::QuinnAcceptor::new(
            endpoint,
            addr,
            tokio_util::sync::CancellationToken::new(),
        );
        let server = Running::start(acceptor);
        let tls = pki.client(Some((&pki.alice, &pki)), b"h3");
        let client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap(),
        ));
        let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        endpoint.set_default_client_config(client_config);
        let conn = endpoint.connect(addr, "localhost").unwrap().await.unwrap();
        let (mut driver, mut sender) =
            salvo_http3::client::new(salvo_http3::quinn::Connection::new(conn.clone()))
                .await
                .unwrap();
        let driver_task = tokio::spawn(async move { driver.wait_idle().await });
        let mut observations = Vec::new();
        for _ in 0..2 {
            let mut stream = sender
                .send_request(http::Request::get("https://localhost/").body(()).unwrap())
                .await
                .unwrap();
            stream.finish().await.unwrap();
            let response = stream.recv_response().await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let mut body = Vec::new();
            while let Some(mut data) = stream.recv_data().await.unwrap() {
                body.extend_from_slice(&data.copy_to_bytes(data.remaining()));
            }
            let info: Value = serde_json::from_slice(&body).unwrap();
            assert_info(&info, &pki, Some(&pki.alice), b"h3");
            observations.push(info);
        }
        assert_eq!(observations[0], observations[1]);
        conn.close(0u32.into(), b"done");
        driver_task.abort();
        server.stop().await;
    }
}
