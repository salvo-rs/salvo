//! Local mock issuer evidence; no public CA is contacted.
#![allow(missing_docs, clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use rcgen::{CertificateParams, CertificateSigningRequestParams, CertifiedIssuer, IsCa, KeyPair};
use salvo_acme::{AcmeListener, CertIssuer, IssuedCertificate, OcspConfig};
use salvo_core::conn::Listener;
use salvo_core::conn::tcp::TcpListener;

struct LocalIssuer {
    ca: CertifiedIssuer<'static, KeyPair>,
}
#[salvo_core::async_trait]
impl CertIssuer for LocalIssuer {
    async fn issue(
        &self,
        csr: &[u8],
        _: &[String],
    ) -> salvo_acme::certon::Result<IssuedCertificate> {
        let request = CertificateSigningRequestParams::from_der(&csr.to_vec().into()).unwrap();
        let certificate = request.signed_by(&self.ca).unwrap();
        Ok(IssuedCertificate {
            certificate_pem: format!("{}{}", certificate.pem(), self.ca.pem()).into_bytes(),
            private_key_pem: Vec::new(),
            metadata: serde_json::json!({}),
        })
    }
    fn issuer_key(&self) -> String {
        "audit-middleware-lifecycle".to_owned()
    }
}

#[tokio::test]
async fn last_server_config_owner_stops_certificate_maintenance() {
    let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let ca = CertifiedIssuer::self_signed(ca_params, KeyPair::generate().unwrap()).unwrap();
    let issuer = Arc::new(LocalIssuer { ca });
    let weak = Arc::downgrade(&issuer);
    let directory = tempfile::tempdir().unwrap();
    let acceptor = tokio::time::timeout(
        Duration::from_secs(5),
        TcpListener::new("127.0.0.1:0")
            .acme()
            .directory("audit", "http://127.0.0.1:1/directory")
            .cache_path(directory.path())
            .add_domain("bootstrap.example")
            .ocsp(OcspConfig {
                disable_stapling: true,
                ..Default::default()
            })
            .add_issuer(issuer.clone())
            .try_bind(),
    )
    .await
    .unwrap()
    .unwrap();
    let config = acceptor.server_config();
    drop(issuer);
    drop(acceptor);
    assert!(
        weak.upgrade().is_some(),
        "an exported config still owns certificate maintenance"
    );
    drop(config);
    // Give owned task cancellation a chance to finish after the final config drops.
    tokio::time::timeout(Duration::from_secs(5), async {
        while weak.upgrade().is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
