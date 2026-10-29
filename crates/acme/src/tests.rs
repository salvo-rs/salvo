//! Local CA integration: no production ACME endpoints or privileged ports.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rcgen::{CertificateParams, CertificateSigningRequestParams, CertifiedIssuer, IsCa, KeyPair};
use salvo_core::conn::tcp::{TcpAcceptor, TcpListener};
use salvo_core::conn::{Accepted, Acceptor, Holding, Listener};
use salvo_core::fuse::ArcFusePolicy;
use salvo_core::http::StatusCode;
use salvo_core::writing::Text;
use salvo_core::{Depot, FlowCtrl, Handler, Request, Response, Router, Server, async_trait};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{
    ClientConfig, DigitallySignedStruct, Error, RootCertStore, SignatureScheme,
};
use x509_parser::prelude::FromDer;

use crate::listener::default_crypto_provider;
use crate::{AcmeListenerBuilder, ChallengeType};

#[derive(Debug)]
struct BoundListener(TcpAcceptor, usize, Arc<AtomicUsize>);

#[derive(Debug)]
struct FaultyAcceptor {
    inner: TcpAcceptor,
    errors_left: usize,
    observed: Arc<AtomicUsize>,
}

impl Acceptor for FaultyAcceptor {
    type Coupler = <TcpAcceptor as Acceptor>::Coupler;
    type Stream = <TcpAcceptor as Acceptor>::Stream;

    fn holdings(&self) -> &[Holding] {
        self.inner.holdings()
    }

    async fn accept(
        &mut self,
        policy: Option<ArcFusePolicy>,
    ) -> std::io::Result<Accepted<Self::Coupler, Self::Stream>> {
        if self.errors_left > 0 {
            self.errors_left -= 1;
            self.observed.fetch_add(1, Ordering::SeqCst);
            return Err(std::io::Error::from(std::io::ErrorKind::ConnectionAborted));
        }
        self.inner.accept(policy).await
    }
}

impl Listener for BoundListener {
    type Acceptor = FaultyAcceptor;

    async fn try_bind(self) -> salvo_core::Result<Self::Acceptor> {
        Ok(FaultyAcceptor {
            inner: self.0,
            errors_left: self.1,
            observed: self.2,
        })
    }
}

struct ServerTask(tokio::task::JoinHandle<()>);

impl Drop for ServerTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct MockCa {
    base: String,
    tls_addr: SocketAddr,
    http_base: Option<String>,
    key_auth: Mutex<String>,
    certificate: Mutex<String>,
    issuer: CertifiedIssuer<'static, KeyPair>,
    validated: AtomicBool,
    selected: Mutex<Vec<String>>,
}

struct MockCaHandler(Arc<MockCa>);

impl std::ops::Deref for MockCaHandler {
    type Target = MockCa;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl Handler for MockCaHandler {
    async fn handle(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        res.headers_mut()
            .insert("replay-nonce", "local-test-nonce".parse().unwrap());
        let value = match req.uri().path() {
            "/directory" => {
                json!({"newNonce":format!("{}/nonce", self.base), "newAccount":format!("{}/account", self.base), "newOrder":format!("{}/order", self.base), "revokeCert":format!("{}/revoke", self.base), "keyChange":format!("{}/key", self.base)})
            }
            "/nonce" => json!({}),
            "/account" => {
                let envelope: Value = serde_json::from_slice(req.payload().await.unwrap()).unwrap();
                let protected: Value = serde_json::from_slice(
                    &URL_SAFE_NO_PAD
                        .decode(envelope["protected"].as_str().unwrap())
                        .unwrap(),
                )
                .unwrap();
                let jwk = &protected["jwk"];
                // RFC 7638 canonical EC members, independent of serde map features.
                let canonical = format!(
                    r#"{{"crv":"{}","kty":"{}","x":"{}","y":"{}"}}"#,
                    jwk["crv"].as_str().unwrap(),
                    jwk["kty"].as_str().unwrap(),
                    jwk["x"].as_str().unwrap(),
                    jwk["y"].as_str().unwrap()
                );
                *self.key_auth.lock().await = format!(
                    "local-token.{}",
                    URL_SAFE_NO_PAD.encode(Sha256::digest(canonical.as_bytes()))
                );
                res.headers_mut().insert(
                    "location",
                    format!("{}/account/1", self.base).parse().unwrap(),
                );
                json!({"status":"valid"})
            }
            "/order" => {
                res.headers_mut().insert(
                    "location",
                    format!("{}/order/1", self.base).parse().unwrap(),
                );
                json!({"status":"pending", "authorizations":[format!("{}/authorization", self.base)], "finalize":format!("{}/finalize", self.base)})
            }
            "/authorization" => {
                json!({"status":if self.validated.load(Ordering::SeqCst) { "valid" } else { "pending" }, "identifier":{"type":"dns","value":"example.com"}, "challenges":[{"type":"http-01","token":"local-token","url":format!("{}/challenge/http", self.base)}, {"type":"tls-alpn-01","token":"local-token","url":format!("{}/challenge/tls", self.base)}]})
            }
            path @ ("/challenge/http" | "/challenge/tls") => {
                self.selected.lock().await.push(path.into());
                let key_auth = self.key_auth.lock().await.clone();
                if path.ends_with("http") {
                    let response = reqwest::get(format!(
                        "{}/.well-known/acme-challenge/local-token",
                        self.http_base
                            .as_ref()
                            .expect("HTTP mode starts challenge router")
                    ))
                    .await
                    .unwrap();
                    assert_eq!(response.status().as_u16(), 200);
                    assert_eq!(response.text().await.unwrap(), key_auth);
                } else {
                    challenge_handshake(self.tls_addr, &key_auth).await.unwrap();
                }
                self.validated.store(true, Ordering::SeqCst);
                json!({"type":if path.ends_with("http") { "http-01" } else { "tls-alpn-01" }, "url":format!("{}{}", self.base, path), "token":"local-token", "status":"valid"})
            }
            "/finalize" => {
                let envelope: Value = serde_json::from_slice(req.payload().await.unwrap()).unwrap();
                let payload: Value = serde_json::from_slice(
                    &URL_SAFE_NO_PAD
                        .decode(envelope["payload"].as_str().unwrap())
                        .unwrap(),
                )
                .unwrap();
                let csr = URL_SAFE_NO_PAD
                    .decode(payload["csr"].as_str().unwrap())
                    .unwrap();
                let csr = CertificateSigningRequestParams::from_der(&csr.into()).unwrap();
                *self.certificate.lock().await = csr.signed_by(&self.issuer).unwrap().pem();
                json!({"status":"valid","certificate":format!("{}/certificate", self.base)})
            }
            "/certificate" => {
                res.render(Text::Plain(self.certificate.lock().await.clone()));
                return;
            }
            path => panic!("unexpected local CA request: {path}"),
        };
        res.render(Text::Json(value.to_string()));
    }
}

#[derive(Debug)]
struct ChallengeVerifier([u8; 32]);

impl ServerCertVerifier for ChallengeVerifier {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let invalid = || Error::General("invalid ACME challenge certificate".into());
        if name.to_str() != "example.com" {
            return Err(invalid());
        }
        let (_, cert) = x509_parser::certificate::X509Certificate::from_der(cert.as_ref())
            .map_err(|_| invalid())?;
        let extension = cert
            .extensions()
            .iter()
            .find(|extension| extension.oid.to_id_string() == "1.3.6.1.5.5.7.1.31")
            .ok_or_else(invalid)?;
        let mut expected = vec![0x04, 0x20];
        expected.extend_from_slice(&self.0);
        let names = cert
            .subject_alternative_name()
            .map_err(|_| invalid())?
            .ok_or_else(invalid)?;
        if !extension.critical
            || extension.value != expected
            || names.value.general_names
                != [x509_parser::extensions::GeneralName::DNSName("example.com")]
        {
            return Err(invalid());
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_challenge_signature(message, cert, signature)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_challenge_signature(message, cert, signature)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        default_crypto_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

fn verify_challenge_signature(
    message: &[u8],
    cert: &CertificateDer<'_>,
    signature: &DigitallySignedStruct,
) -> Result<HandshakeSignatureValid, Error> {
    let (_, cert) = x509_parser::certificate::X509Certificate::from_der(cert.as_ref())
        .map_err(|_| Error::General("invalid challenge certificate".into()))?;
    for (scheme, algorithms) in default_crypto_provider()
        .signature_verification_algorithms
        .mapping
    {
        if *scheme == signature.scheme
            && algorithms.iter().any(|algorithm| {
                algorithm
                    .verify_signature(
                        cert.public_key().subject_public_key.data.as_ref(),
                        message,
                        signature.signature(),
                    )
                    .is_ok()
            })
        {
            return Ok(HandshakeSignatureValid::assertion());
        }
    }
    Err(Error::General(
        "invalid challenge handshake signature".into(),
    ))
}

async fn challenge_handshake(addr: SocketAddr, key_auth: &str) -> std::io::Result<()> {
    let mut config = ClientConfig::builder_with_provider(default_crypto_provider())
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(ChallengeVerifier(
            Sha256::digest(key_auth.as_bytes()).into(),
        )))
        .with_no_client_auth();
    config.alpn_protocols = vec![b"acme-tls/1".to_vec()];
    let stream = tokio::net::TcpStream::connect(addr).await?;
    let stream = TlsConnector::from(Arc::new(config))
        .connect(ServerName::try_from("example.com").unwrap(), stream)
        .await?;
    assert_eq!(
        stream.get_ref().1.alpn_protocol(),
        Some(b"acme-tls/1".as_slice())
    );
    Ok(())
}

async fn initial_issuance(
    challenge_type: ChallengeType,
    accept_errors: usize,
    check_cached_restart: bool,
) {
    let tls = TcpListener::new("127.0.0.1:0").try_bind().await.unwrap();
    let tls_addr = tls.local_addr().unwrap();
    let ca = TcpListener::new("127.0.0.1:0").try_bind().await.unwrap();
    let base = format!("http://{}", ca.local_addr().unwrap());
    let directory = tempfile::tempdir().unwrap();
    let observed = Arc::new(AtomicUsize::new(0));
    let mut listener =
        AcmeListenerBuilder::new(BoundListener(tls, accept_errors, observed.clone()))
            .add_domain("example.com")
            .directory("local", format!("{base}/directory"))
            .cache_path(directory.path());
    let mut http_task = None;
    let mut http_base = None;
    if challenge_type == ChallengeType::Http01 {
        let mut router = Router::new();
        listener = listener.http01_challenge(&mut router);
        let http = TcpListener::new("127.0.0.1:0").try_bind().await.unwrap();
        http_base = Some(format!("http://{}", http.local_addr().unwrap()));
        http_task = Some(ServerTask(tokio::spawn(Server::new(http).serve(router))));
    }
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let state = Arc::new(MockCa {
        base,
        tls_addr,
        http_base,
        key_auth: Mutex::new(String::new()),
        certificate: Mutex::new(String::new()),
        issuer: CertifiedIssuer::self_signed(ca_params, KeyPair::generate().unwrap()).unwrap(),
        validated: AtomicBool::new(false),
        selected: Mutex::new(Vec::new()),
    });
    let _ca_task = ServerTask(tokio::spawn(
        Server::new(ca).serve(Router::with_path("{**}").goal(MockCaHandler(state.clone()))),
    ));
    let acceptor = tokio::time::timeout(std::time::Duration::from_secs(15), listener.try_bind())
        .await
        .unwrap()
        .unwrap();
    assert!(state.validated.load(Ordering::SeqCst));
    assert_eq!(observed.load(Ordering::SeqCst), accept_errors);
    assert_eq!(
        *state.selected.lock().await,
        [if challenge_type == ChallengeType::Http01 {
            "/challenge/http"
        } else {
            "/challenge/tls"
        }]
    );
    if let Some(base) = &state.http_base {
        let response = reqwest::get(format!("{base}/.well-known/acme-challenge/local-token"))
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), StatusCode::NOT_FOUND.as_u16());
    }
    let mut server = ServerTask(tokio::spawn(Server::new(acceptor).serve(Router::new())));
    let mut roots = RootCertStore::empty();
    roots.add(state.issuer.der().clone()).unwrap();
    let config = Arc::new(
        ClientConfig::builder_with_provider(default_crypto_provider())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    );
    let stream = tokio::net::TcpStream::connect(tls_addr).await.unwrap();
    TlsConnector::from(config.clone())
        .connect(ServerName::try_from("example.com").unwrap(), stream)
        .await
        .unwrap();
    // The temporary certificate must be removed after CA validation.
    let error = challenge_handshake(tls_addr, &state.key_auth.lock().await)
        .await
        .unwrap_err();
    assert!(
        matches!(
            error
                .get_ref()
                .and_then(|error| error.downcast_ref::<Error>()),
            Some(Error::AlertReceived(
                tokio_rustls::rustls::AlertDescription::AccessDenied
            ))
        ),
        "{error:?}"
    );
    if check_cached_restart {
        server.0.abort();
        assert!((&mut server.0).await.unwrap_err().is_cancelled());
        let tcp = TcpListener::new("127.0.0.1:0").try_bind().await.unwrap();
        let addr = tcp.local_addr().unwrap();
        let observed = Arc::new(AtomicUsize::new(0));
        let listener = AcmeListenerBuilder::new(BoundListener(tcp, 1, observed.clone()))
            .add_domain("example.com")
            .directory("local", format!("{}/directory", state.base))
            .cache_path(directory.path());
        let acceptor = tokio::time::timeout(std::time::Duration::from_secs(2), listener.try_bind())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(observed.load(Ordering::SeqCst), 1);
        let _cached_server = ServerTask(tokio::spawn(Server::new(acceptor).serve(Router::new())));
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        TlsConnector::from(config)
            .connect(ServerName::try_from("example.com").unwrap(), stream)
            .await
            .unwrap();
        // Cached startup does not need another CA order or challenge.
        assert_eq!(state.selected.lock().await.len(), 1);
    }
    drop(http_task);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_tls_alpn_initial_issuance_and_cleanup() {
    initial_issuance(ChallengeType::TlsAlpn01, 0, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn built_in_http01_initial_issuance_and_cleanup() {
    initial_issuance(ChallengeType::Http01, 0, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transient_accept_errors_preserve_initial_issuance_and_cached_certificates() {
    initial_issuance(ChallengeType::TlsAlpn01, 1, true).await;
}
