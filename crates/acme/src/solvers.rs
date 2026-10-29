//! Challenge presentation through Salvo's existing HTTP router and TLS resolver.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use certon::handshake::CertResolver;
use certon::solvers::Solver;
use rcgen::{CertificateParams, CustomExtension, KeyPair, PKCS_ECDSA_P256_SHA256};
use salvo_core::async_trait;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use tokio_rustls::rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_rustls::rustls::sign::CertifiedKey;

struct Presentation {
    solver: Arc<dyn Solver>,
    domain: String,
    token: String,
    key_auth: String,
}

type Presentations = Arc<Mutex<Vec<Presentation>>>;

tokio::task_local! {
    static PRESENTATIONS: Presentations;
}

/// Track only Salvo's built-in solvers; custom solvers retain their own contract.
pub(crate) struct TrackedSolver(pub(crate) Arc<dyn Solver>);

#[async_trait]
impl Solver for TrackedSolver {
    async fn present(&self, domain: &str, token: &str, key_auth: &str) -> certon::Result<()> {
        self.0.present(domain, token, key_auth).await?;
        let _ = PRESENTATIONS.try_with(|presentations| {
            presentations
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(Presentation {
                    solver: self.0.clone(),
                    domain: domain.into(),
                    token: token.into(),
                    key_auth: key_auth.into(),
                });
        });
        Ok(())
    }

    async fn wait(&self, domain: &str, token: &str, key_auth: &str) -> certon::Result<()> {
        self.0.wait(domain, token, key_auth).await
    }

    async fn cleanup(&self, domain: &str, token: &str, key_auth: &str) -> certon::Result<()> {
        self.0.cleanup(domain, token, key_auth).await?;
        let _ = PRESENTATIONS.try_with(|presentations| {
            presentations
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .retain(|entry| {
                    !(entry.domain == domain
                        && entry.token == token
                        && Arc::ptr_eq(&entry.solver, &self.0))
                });
        });
        Ok(())
    }
}

struct CleanupGuard {
    entries: Presentations,
    runtime: tokio::runtime::Handle,
}

impl CleanupGuard {
    fn start_cleanup(&self) -> Option<tokio::task::JoinHandle<()>> {
        let entries = std::mem::take(
            &mut *self
                .entries
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        );
        if entries.is_empty() {
            return None;
        }
        // Own cleanup independently of the issuer future: cancelling either the
        // network exchange or its cleanup wait must not strand a presentation.
        Some(self.runtime.spawn(async move {
            for entry in entries {
                if let Err(error) = entry
                    .solver
                    .cleanup(&entry.domain, &entry.token, &entry.key_auth)
                    .await
                {
                    tracing::warn!(error = ?error, "ACME challenge cleanup failed");
                }
            }
        }))
    }
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        let _ = self.start_cleanup();
    }
}

/// Certon's acceptance-error path returns before calling solver cleanup. Scope
/// every issuance (including maintenance renewals) so temporary presentations
/// are also removed on that path and when the issuance future is cancelled.
pub(crate) struct AcmeIssuer<I = certon::AcmeIssuer>(pub(crate) I);

#[async_trait]
impl<I: certon::CertIssuer> certon::CertIssuer for AcmeIssuer<I> {
    async fn issue(
        &self,
        csr: &[u8],
        domains: &[String],
    ) -> certon::Result<certon::IssuedCertificate> {
        let entries = Arc::new(Mutex::new(Vec::new()));
        let guard = CleanupGuard {
            entries: entries.clone(),
            runtime: tokio::runtime::Handle::current(),
        };
        let result = PRESENTATIONS
            .scope(entries, self.0.issue(csr, domains))
            .await;
        if let Some(cleanup) = guard.start_cleanup() {
            let _ = cleanup.await;
        }
        result
    }

    fn issuer_key(&self) -> String {
        self.0.issuer_key()
    }

    fn as_revoker(&self) -> Option<&dyn certon::Revoker> {
        self.0.as_revoker()
    }
}

pub(crate) struct Http01Solver {
    pub(crate) keys: Arc<RwLock<HashMap<String, String>>>,
}

#[async_trait]
impl Solver for Http01Solver {
    async fn present(&self, _domain: &str, token: &str, key_auth: &str) -> certon::Result<()> {
        self.keys
            .write()
            .await
            .insert(token.into(), key_auth.into());
        Ok(())
    }

    async fn cleanup(&self, _domain: &str, token: &str, _key_auth: &str) -> certon::Result<()> {
        self.keys.write().await.remove(token);
        Ok(())
    }
}

pub(crate) struct TlsAlpn01Solver {
    pub(crate) resolver: Arc<CertResolver>,
}

#[async_trait]
impl Solver for TlsAlpn01Solver {
    async fn present(&self, domain: &str, _token: &str, key_auth: &str) -> certon::Result<()> {
        let build_cert = || -> Result<CertifiedKey, Box<dyn std::error::Error + Send + Sync>> {
            let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?;
            let mut params = CertificateParams::new(vec![domain.into()])?;
            // RFC 8737: critical acmeIdentifier containing the DER OCTET STRING
            // of SHA-256(keyAuthorization), not the token or a plain-text digest.
            let mut value = vec![0x04, 0x20];
            value.extend_from_slice(&Sha256::digest(key_auth.as_bytes()));
            let mut extension =
                CustomExtension::from_oid_content(&[1, 3, 6, 1, 5, 5, 7, 1, 31], value);
            extension.set_criticality(true);
            params.custom_extensions.push(extension);
            let cert = params.self_signed(&key)?;
            let signing_key = super::listener::default_crypto_provider()
                .key_provider
                .load_private_key(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
                    key.serialize_der(),
                )))?;
            // `from_der` also performs a WebPKI key-match check, which rejects
            // this intentionally critical ACME-only extension. The certificate
            // and signing key above are generated from the same key pair.
            Ok(CertifiedKey::new(vec![cert.der().clone()], signing_key))
        };
        let cert = build_cert().map_err(|error| certon::Error::Other(error.to_string()))?;
        self.resolver
            .set_challenge_cert(domain.into(), Arc::new(cert))
            .await;
        Ok(())
    }

    async fn cleanup(&self, domain: &str, _token: &str, _key_auth: &str) -> certon::Result<()> {
        self.resolver.remove_challenge_cert(domain).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use certon::CertIssuer;
    use tokio::sync::Notify;

    use super::*;

    struct Issuer {
        solver: Arc<dyn Solver>,
        presented: Arc<Notify>,
        pending: bool,
    }

    #[async_trait]
    impl CertIssuer for Issuer {
        async fn issue(
            &self,
            _csr: &[u8],
            domains: &[String],
        ) -> certon::Result<certon::IssuedCertificate> {
            self.solver
                .present(&domains[0], &domains[0], "proof")
                .await?;
            self.presented.notify_one();
            if self.pending {
                std::future::pending::<()>().await;
            }
            // Model the certon accept_challenge error path that skips cleanup.
            Err(certon::Error::Other("challenge acceptance failed".into()))
        }

        fn issuer_key(&self) -> String {
            "test-issuer".into()
        }
    }

    fn issuer(
        pending: bool,
    ) -> (
        AcmeIssuer<Issuer>,
        Arc<RwLock<HashMap<String, String>>>,
        Arc<Notify>,
    ) {
        let keys = Arc::new(RwLock::new(HashMap::new()));
        let presented = Arc::new(Notify::new());
        let solver = Arc::new(TrackedSolver(Arc::new(Http01Solver { keys: keys.clone() })));
        (
            AcmeIssuer(Issuer {
                solver,
                presented: presented.clone(),
                pending,
            }),
            keys,
            presented,
        )
    }

    #[tokio::test]
    async fn issuer_errors_clean_presentations_on_every_call() {
        let (issuer, keys, _) = issuer(false);
        // Initial issuance and subsequent maintenance issuance use the same wrapper.
        for _ in 0..2 {
            assert!(issuer.issue(&[], &["example.com".into()]).await.is_err());
            assert!(keys.read().await.is_empty());
        }
        assert_eq!(issuer.issuer_key(), "test-issuer");
        assert!(issuer.as_revoker().is_none());
    }

    #[tokio::test]
    async fn cancelling_an_issuance_cleans_only_its_presentations() {
        let (issuer, keys, presented) = issuer(true);
        let issuer = Arc::new(issuer);
        let first = {
            let issuer = issuer.clone();
            tokio::spawn(async move { issuer.issue(&[], &["first.example".into()]).await })
        };
        presented.notified().await;
        let second = {
            let issuer = issuer.clone();
            tokio::spawn(async move { issuer.issue(&[], &["second.example".into()]).await })
        };
        presented.notified().await;
        first.abort();
        assert!(first.await.unwrap_err().is_cancelled());
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while keys.read().await.contains_key("first.example") {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(keys.read().await.contains_key("second.example"));
        second.abort();
        assert!(second.await.unwrap_err().is_cancelled());
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !keys.read().await.is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[test]
    fn dropping_issuance_outside_runtime_still_schedules_cleanup() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (issuer, keys, presented) = issuer(true);
        let domains = ["example.com".into()];
        let mut issuance = Box::pin(issuer.issue(&[], &domains));
        runtime.block_on(async {
            tokio::select! {
                _ = &mut issuance => panic!("test issuance must remain pending"),
                _ = presented.notified() => {}
            }
        });
        drop(issuance);
        runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while !keys.read().await.is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
        });
    }
}
