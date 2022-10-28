//! rustls module
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Error as IoError, ErrorKind, Read, Result as IoResult};
use std::path::Path;
use std::sync::Arc;

use futures_util::future::{ready, Ready};
use futures_util::stream::{once, Once, Stream};
pub use tokio_rustls::rustls::server::ServerConfig;
use tokio_rustls::rustls::server::{
    AllowAnyAnonymousOrAuthenticatedClient, AllowAnyAuthenticatedClient, ClientHello, NoClientAuth, ResolvesServerCert,
};
use tokio_rustls::rustls::sign::{self, CertifiedKey};
use tokio_rustls::rustls::{Certificate, PrivateKey};

use crate::conn::IntoConfigStream;

use super::read_trust_anchor;

/// Private key and certificate
#[derive(Clone, Debug)]
pub struct Keycert {
    key: Vec<u8>,
    cert: Vec<u8>,
    ocsp_resp: Vec<u8>,
}

impl Default for Keycert {
    fn default() -> Self {
        Self::new()
    }
}

impl Keycert {
    /// Create a new keycert.
    #[inline]
    pub fn new() -> Self {
        Self {
            key: vec![],
            cert: vec![],
            ocsp_resp: vec![],
        }
    }
    /// Sets the Tls private key via File Path, returns [`IoError`] if the file cannot be open.
    #[inline]
    pub fn key_from_path(mut self, path: impl AsRef<Path>) -> IoResult<Self> {
        let mut file = File::open(path.as_ref())?;
        file.read_to_end(&mut self.key)?;
        Ok(self)
    }

    /// Sets the Tls private key via bytes slice.
    #[inline]
    pub fn with_key(mut self, key: impl Into<Vec<u8>>) -> Self {
        self.key = key.into();
        self
    }

    /// Specify the file path for the TLS certificate to use.
    #[inline]
    pub fn cert_from_path(mut self, path: impl AsRef<Path>) -> IoResult<Self> {
        let mut file = File::open(path)?;
        file.read_to_end(&mut self.cert)?;
        Ok(self)
    }

    /// Sets the Tls certificate via bytes slice
    #[inline]
    pub fn with_cert(mut self, cert: impl Into<Vec<u8>>) -> Self {
        self.cert = cert.into();
        self
    }

    /// Get the private key.
    #[inline]
    pub fn key(&mut self) -> io::Result<&[u8]> {
        if self.key.is_empty() {
            Err(IoError::new(ErrorKind::Other, "empty key"))
        } else {
            Ok(&self.key)
        }
    }

    /// Get the cert.
    #[inline]
    pub fn cert(&mut self) -> io::Result<&[u8]> {
        if self.cert.is_empty() {
            Err(IoError::new(ErrorKind::Other, "empty cert"))
        } else {
            Ok(&self.cert)
        }
    }

    /// Get ocsp_resp.
    #[inline]
    pub fn ocsp_resp(&self) -> &[u8] {
        &self.ocsp_resp
    }

    fn build_certified_key(&mut self) -> io::Result<CertifiedKey> {
        let cert = rustls_pemfile::certs(&mut self.cert()?)
            .map(|certs| certs.into_iter().map(Certificate).collect())
            .map_err(|_| IoError::new(ErrorKind::Other, "failed to parse tls certificates"))?;

        let key = {
            let mut pkcs8 = rustls_pemfile::pkcs8_private_keys(&mut self.key()?)
                .map_err(|_| IoError::new(ErrorKind::Other, "failed to parse tls private keys"))?;
            if !pkcs8.is_empty() {
                PrivateKey(pkcs8.remove(0))
            } else {
                let mut rsa = rustls_pemfile::rsa_private_keys(&mut self.key()?)
                    .map_err(|_| IoError::new(ErrorKind::Other, "failed to parse tls private keys"))?;

                if !rsa.is_empty() {
                    PrivateKey(rsa.remove(0))
                } else {
                    return Err(IoError::new(ErrorKind::Other, "failed to parse tls private keys"));
                }
            }
        };

        let key = sign::any_supported_type(&key).map_err(|_| IoError::new(ErrorKind::Other, "invalid private key"))?;

        Ok(CertifiedKey {
            cert,
            key,
            ocsp: if !self.ocsp_resp.is_empty() {
                Some(self.ocsp_resp.clone())
            } else {
                None
            },
            sct_list: None,
        })
    }
}

/// Tls client authentication configuration.
#[derive(Clone, Debug)]
pub(crate) enum TlsClientAuth {
    /// No client auth.
    Off,
    /// Allow any anonymous or authenticated client.
    Optional(Vec<u8>),
    /// Allow any authenticated client.
    Required(Vec<u8>),
}

/// Builder to set the configuration for the Tls server.
#[derive(Clone, Debug)]
pub struct RustlsConfig {
    fallback: Option<Keycert>,
    keycerts: HashMap<String, Keycert>,
    client_auth: TlsClientAuth,
    alpn_protocols: Vec<Vec<u8>>,
}

impl RustlsConfig {
    /// Create new `RustlsConfig`
    #[inline]
    pub fn new(fallback: impl Into<Option<Keycert>>) -> Self {
        RustlsConfig {
            fallback: fallback.into(),
            keycerts: HashMap::new(),
            client_auth: TlsClientAuth::Off,
            alpn_protocols: vec![b"h2".to_vec(), b"http/1.1".to_vec()],
        }
    }

    /// Sets the trust anchor for optional Tls client authentication via file path.
    ///
    /// Anonymous and authenticated clients will be accepted. If no trust anchor is provided by any
    /// of the `client_auth_` methods, then client authentication is disabled by default.
    #[inline]
    pub fn client_auth_optional_path(mut self, path: impl AsRef<Path>) -> io::Result<Self> {
        let mut data = vec![];
        let mut file = File::open(path)?;
        file.read_to_end(&mut data)?;
        self.client_auth = TlsClientAuth::Optional(data);
        Ok(self)
    }

    /// Sets the trust anchor for optional Tls client authentication via bytes slice.
    ///
    /// Anonymous and authenticated clients will be accepted. If no trust anchor is provided by any
    /// of the `client_auth_` methods, then client authentication is disabled by default.
    pub fn client_auth_optional(mut self, trust_anchor: impl Into<Vec<u8>>) -> Self {
        self.client_auth = TlsClientAuth::Optional(trust_anchor.into());
        self
    }

    /// Sets the trust anchor for required Tls client authentication via file path.
    ///
    /// Only authenticated clients will be accepted. If no trust anchor is provided by any of the
    /// `client_auth_` methods, then client authentication is disabled by default.
    #[inline]
    pub fn client_auth_required_path(mut self, path: impl AsRef<Path>) -> io::Result<Self> {
        let mut data = vec![];
        let mut file = File::open(path)?;
        file.read_to_end(&mut data)?;
        self.client_auth = TlsClientAuth::Required(data);
        Ok(self)
    }

    /// Sets the trust anchor for required Tls client authentication via bytes slice.
    ///
    /// Only authenticated clients will be accepted. If no trust anchor is provided by any of the
    /// `client_auth_` methods, then client authentication is disabled by default.
    #[inline]
    pub fn client_auth_required(mut self, trust_anchor: impl Into<Vec<u8>>) -> Self {
        self.client_auth = TlsClientAuth::Required(trust_anchor.into());
        self
    }

    /// Add a new keycert to be used for the given SNI `name`.
    #[inline]
    pub fn keycert(mut self, name: impl Into<String>, keycert: Keycert) -> Self {
        self.keycerts.insert(name.into(), keycert);
        self
    }

    /// Add a new keycert to be used for the given SNI `name`.
    #[inline]
    pub fn alpn_protocols(mut self, alpn_protocols: impl Into<Vec<Vec<u8>>>) -> Self {
        self.alpn_protocols = alpn_protocols.into();
        self
    }

    /// ServerConfig
    pub(crate) fn build_server_config(mut self) -> io::Result<ServerConfig> {
        let fallback = self
            .fallback
            .as_mut()
            .map(|fallback| fallback.build_certified_key())
            .transpose()?
            .map(Arc::new);
        let mut certified_keys = HashMap::new();

        for (name, keycert) in &mut self.keycerts {
            certified_keys.insert(name.clone(), Arc::new(keycert.build_certified_key()?));
        }

        let client_auth = match &self.client_auth {
            TlsClientAuth::Off => NoClientAuth::new(),
            TlsClientAuth::Optional(trust_anchor) => {
                AllowAnyAnonymousOrAuthenticatedClient::new(read_trust_anchor(trust_anchor)?)
            }
            TlsClientAuth::Required(trust_anchor) => AllowAnyAuthenticatedClient::new(read_trust_anchor(trust_anchor)?),
        };

        let mut config = ServerConfig::builder()
            .with_safe_defaults()
            .with_client_cert_verifier(client_auth)
            .with_cert_resolver(Arc::new(CertResolver {
                certified_keys,
                fallback,
            }));
        config.alpn_protocols = self.alpn_protocols;
        Ok(config)
    }
}

pub(crate) struct CertResolver {
    fallback: Option<Arc<CertifiedKey>>,
    certified_keys: HashMap<String, Arc<CertifiedKey>>,
}

impl ResolvesServerCert for CertResolver {
    fn resolve(&self, client_hello: ClientHello) -> Option<Arc<CertifiedKey>> {
        client_hello
            .server_name()
            .and_then(|name| self.certified_keys.get(name).map(Arc::clone))
            .or_else(|| self.fallback.clone())
    }
}

impl IntoConfigStream<RustlsConfig> for RustlsConfig {
    type Stream = Once<Ready<RustlsConfig>>;

    fn into_stream(self) -> Self::Stream {
        once(ready(self))
    }
}

impl<T> IntoConfigStream<RustlsConfig> for T
where
    T: Stream<Item = RustlsConfig> + Send + 'static,
{
    type Stream = T;

    fn into_stream(self) -> Self {
        self
    }
}
