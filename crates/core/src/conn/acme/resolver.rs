use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::RwLock;
use tokio_rustls::rustls::server::{ClientHello, ResolvesServerCert};
use tokio_rustls::rustls::sign::CertifiedKey;
use x509_parser::prelude::{FromDer, X509Certificate};

pub(crate) const ACME_TLS_ALPN_NAME: &[u8] = b"acme-tls/1";

#[derive(Default, Debug)]
pub(crate) struct ResolveServerCert {
    pub(crate) cert: RwLock<Option<Arc<CertifiedKey>>>,
    pub(crate) acme_keys: RwLock<HashMap<String, Arc<CertifiedKey>>>,
}

impl ResolveServerCert {
    #[inline]
    pub(crate) fn will_expired(&self, before: Duration) -> bool {
        let cert = self.cert.read();
        match cert
            .as_ref()
            .and_then(|cert| cert.cert.first())
            .and_then(|cert| X509Certificate::from_der(cert.as_ref()).ok())
            .map(|(_, cert)| cert.validity().not_after.timestamp())
        {
            Some(valid_until) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("time went backwards");
                (now + before).as_secs() as i64 > valid_until
            }
            None => true,
        }
    }
}

impl ResolvesServerCert for ResolveServerCert {
    fn resolve(&self, client_hello: ClientHello) -> Option<Arc<CertifiedKey>> {
        if client_hello
            .alpn()
            .and_then(|mut iter| iter.find(|alpn| *alpn == ACME_TLS_ALPN_NAME))
            .is_some()
        {
            return match client_hello.server_name() {
                None => None,
                Some(domain) => {
                    tracing::debug!(domain, "load acme key");
                    if let Some(cert) = self.acme_keys.read().get(domain).cloned() {
                        Some(cert)
                    } else {
                        tracing::error!(domain, "acme key not found");
                        None
                    }
                }
            };
        };

        self.cert.read().as_ref().cloned()
    }
}
