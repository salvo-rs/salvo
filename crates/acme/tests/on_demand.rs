//! Local mock-issuer reproduction; no public ACME service is contacted.
#![allow(missing_docs, clippy::unwrap_used)]

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rcgen::{CertificateParams, CertificateSigningRequestParams, CertifiedIssuer, IsCa, KeyPair};
use salvo_acme::{AcmeListener, CertIssuer, IssuedCertificate, OcspConfig, OnDemandConfig};
use salvo_core::conn::{Acceptor, Listener};
use salvo_core::prelude::*;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};

struct LocalIssuer {
    ca: CertifiedIssuer<'static, KeyPair>,
    calls: Arc<AtomicUsize>,
}

#[salvo_core::async_trait]
impl CertIssuer for LocalIssuer {
    async fn issue(
        &self,
        csr: &[u8],
        _: &[String],
    ) -> salvo_acme::certon::Result<IssuedCertificate> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let request = CertificateSigningRequestParams::from_der(&csr.to_vec().into()).unwrap();
        let certificate = request.signed_by(&self.ca).unwrap();
        Ok(IssuedCertificate {
            certificate_pem: format!("{}{}", certificate.pem(), self.ca.pem()).into_bytes(),
            private_key_pem: Vec::new(),
            metadata: serde_json::json!({}),
        })
    }

    fn issuer_key(&self) -> String {
        "full-audit-local".to_owned()
    }
}

#[handler]
async fn hello() -> &'static str {
    "hello"
}

#[tokio::test]
async fn allowlisted_on_demand_domain_obtains_and_serves_certificate() {
    let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let ca = CertifiedIssuer::self_signed(ca_params, KeyPair::generate().unwrap()).unwrap();
    let mut roots = RootCertStore::empty();
    roots.add(ca.der().clone()).unwrap();
    let client = TlsConnector::from(Arc::new(
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ));
    let calls = Arc::new(AtomicUsize::new(0));
    let storage = tempfile::tempdir().unwrap();
    let acceptor = tokio::time::timeout(
        Duration::from_secs(5),
        TcpListener::new("127.0.0.1:0")
            .acme()
            .directory("audit", "http://127.0.0.1:1/directory")
            .cache_path(storage.path())
            .add_domain("bootstrap.example")
            .ocsp(OcspConfig {
                disable_stapling: true,
                ..Default::default()
            })
            .add_issuer(Arc::new(LocalIssuer {
                ca,
                calls: calls.clone(),
            }))
            .on_demand(Arc::new(OnDemandConfig {
                decision_func: None,
                host_allowlist: Some(HashSet::from(["customer.example".to_owned()])),
                rate_limit: None,
                obtain_func: None,
            }))
            .try_bind(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let addr = acceptor.holdings()[0]
        .local_addr
        .clone()
        .into_std()
        .unwrap();
    let server = tokio::spawn(Server::new(acceptor).serve(Router::new().goal(hello)));
    // Positive control: the initial managed certificate is served successfully.
    let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
    let connected = client
        .connect(ServerName::try_from("bootstrap.example").unwrap(), tcp)
        .await;
    assert!(connected.is_ok(), "{connected:?}");
    drop(connected);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
            if client
                .connect(ServerName::try_from("customer.example").unwrap(), tcp)
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
    assert!(
        client
            .connect(ServerName::try_from("denied.example").unwrap(), tcp)
            .await
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    server.abort();
}
