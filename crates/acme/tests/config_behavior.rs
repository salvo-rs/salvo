//! Listener configuration regressions using a local issuer and no public CA.
#![allow(missing_docs, clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rcgen::{CertificateParams, CertificateSigningRequestParams, CertifiedIssuer, IsCa, KeyPair};
use salvo_acme::{AcmeConfig, AcmeListener, CertIssuer, IssuedCertificate, OcspConfig};
use salvo_core::conn::Listener;
use salvo_core::conn::tcp::TcpListener;

struct LocalIssuer {
    ca: CertifiedIssuer<'static, KeyPair>,
    calls: AtomicUsize,
    expiring_first: bool,
}

impl LocalIssuer {
    fn new(expiring_first: bool) -> Arc<Self> {
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        Arc::new(Self {
            ca: CertifiedIssuer::self_signed(params, KeyPair::generate().unwrap()).unwrap(),
            calls: AtomicUsize::new(0),
            expiring_first,
        })
    }
}

#[salvo_core::async_trait]
impl CertIssuer for LocalIssuer {
    async fn issue(
        &self,
        csr: &[u8],
        _: &[String],
    ) -> salvo_acme::certon::Result<IssuedCertificate> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let mut request = CertificateSigningRequestParams::from_der(&csr.to_vec().into()).unwrap();
        let now = time::OffsetDateTime::now_utc();
        if self.expiring_first && call == 0 {
            request.params.not_before = now - time::Duration::days(2);
            request.params.not_after = now + time::Duration::hours(12);
        } else {
            request.params.not_before = now - time::Duration::minutes(1);
            request.params.not_after = now + time::Duration::days(90);
        }
        let certificate = request.signed_by(&self.ca).unwrap();
        Ok(IssuedCertificate {
            certificate_pem: format!("{}{}", certificate.pem(), self.ca.pem()).into_bytes(),
            private_key_pem: Vec::new(),
            metadata: serde_json::json!({}),
        })
    }

    fn issuer_key(&self) -> String {
        "listener-config-local".into()
    }
}

fn config(issuer: Arc<LocalIssuer>) -> AcmeConfig {
    AcmeConfig::builder()
        .directory("local", "http://127.0.0.1:1/directory")
        .add_domain("config.example")
        .add_issuer(issuer)
        .agree_to_tos(false)
        .ocsp(OcspConfig {
            disable_stapling: true,
            ..Default::default()
        })
        .build()
        .unwrap()
}

#[tokio::test]
async fn public_configuration_without_storage_is_local_to_each_listener() {
    let issuer = LocalIssuer::new(false);
    for expected in 1..=2 {
        let acceptor = tokio::time::timeout(
            Duration::from_secs(5),
            TcpListener::new("127.0.0.1:0")
                .acme()
                .config(config(issuer.clone()))
                .try_bind(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(issuer.calls.load(Ordering::SeqCst), expected);
        drop(acceptor);
    }
}

#[tokio::test]
async fn configured_absolute_window_renews_and_caches_a_replacement() {
    let issuer = LocalIssuer::new(true);
    let acceptor = tokio::time::timeout(
        Duration::from_secs(5),
        TcpListener::new("127.0.0.1:0")
            .acme()
            .config(config(issuer.clone()).before_expired(Duration::from_secs(24 * 60 * 60)))
            .check_duration(Duration::from_millis(20))
            .try_bind(),
    )
    .await
    .unwrap()
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while issuer.calls.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // A cached fresh replacement must not be reissued by subsequent checks.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(issuer.calls.load(Ordering::SeqCst), 2);
    drop(acceptor);
}

#[tokio::test]
async fn listeners_sharing_storage_recheck_before_forced_renewal() {
    let issuer = LocalIssuer::new(true);
    let directory = tempfile::tempdir().unwrap();
    let first = TcpListener::new("127.0.0.1:0")
        .acme()
        .config(config(issuer.clone()).before_expired(Duration::from_secs(24 * 60 * 60)))
        .cache_path(directory.path())
        .check_duration(Duration::from_millis(20));
    let second = TcpListener::new("127.0.0.1:0")
        .acme()
        .config(config(issuer.clone()).before_expired(Duration::from_secs(24 * 60 * 60)))
        .cache_path(directory.path())
        .check_duration(Duration::from_millis(20));
    let (first, second) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(first.try_bind(), second.try_bind())
    })
    .await
    .unwrap();
    let (first, second) = (first.unwrap(), second.unwrap());
    tokio::time::timeout(Duration::from_secs(5), async {
        while issuer.calls.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(issuer.calls.load(Ordering::SeqCst), 2);
    drop((first, second));
}
