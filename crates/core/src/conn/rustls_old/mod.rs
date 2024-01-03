//! `RustlsListener` and utils.
use std::collections::HashMap;
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::sync::Arc;

use tokio_rustls_old::rustls::server::{ClientHello, ResolvesServerCert};
use tokio_rustls_old::rustls::sign::CertifiedKey;
use tokio_rustls_old::rustls::{Certificate, RootCertStore};

pub(crate) fn read_trust_anchor(mut trust_anchor: &[u8]) -> IoResult<RootCertStore> {
    let certs = rustls_pemfile_old::certs(&mut trust_anchor)?;
    let mut store = RootCertStore::empty();
    for cert in certs {
        store
            .add(&Certificate(cert))
            .map_err(|err| IoError::new(ErrorKind::Other, err.to_string()))?;
    }
    Ok(store)
}

pub(crate) struct CertResolver {
    pub(crate) fallback: Option<Arc<CertifiedKey>>,
    pub(crate) certified_keys: HashMap<String, Arc<CertifiedKey>>,
}

impl ResolvesServerCert for CertResolver {
    fn resolve(&self, client_hello: ClientHello) -> Option<Arc<CertifiedKey>> {
        client_hello
            .server_name()
            .and_then(|name| self.certified_keys.get(name).map(Arc::clone))
            .or_else(|| self.fallback.clone())
    }
}
