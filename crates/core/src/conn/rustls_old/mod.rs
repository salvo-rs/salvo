//! `RustlsListener` and utils.
use std::io::{Error as IoError, ErrorKind, Result as IoResult};

use tokio_rustls_old::rustls::{Certificate, RootCertStore};

pub(crate) mod config;
pub use config::{Keycert, RustlsConfig, ServerConfig};

#[inline]
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