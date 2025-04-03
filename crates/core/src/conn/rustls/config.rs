//! rustls module
use std::collections::HashMap;
use std::fs::File;
use std::future::{ready, Ready};
use std::io::{Error as IoError, Read, Result as IoResult};
use std::path::Path;
use std::sync::Arc;

use futures_util::stream::{once, Once, Stream};
use tokio_rustls::rustls::crypto::ring::sign::any_supported_type;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::server::{ClientHello, ResolvesServerCert, WebPkiClientVerifier};
use tokio_rustls::rustls::sign::CertifiedKey;

pub use tokio_rustls::rustls::server::ServerConfig;

use crate::conn::IntoConfigStream;

use super::read_trust_anchor;

/// Private key and certificate
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Keycert {
    /// Private key.
    pub key: Vec<u8>,
    /// Certificate.
    pub cert: Vec<u8>,
    /// OCSP response.
    pub ocsp_resp: Vec<u8>,
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
    pub fn key(mut self, key: impl Into<Vec<u8>>) -> Self {
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
    pub fn cert(mut self, cert: impl Into<Vec<u8>>) -> Self {
        self.cert = cert.into();
        self
    }

    /// Get ocsp_resp.
    #[inline]
    pub fn ocsp_resp(&self) -> &[u8] {
        &self.ocsp_resp
    }

    fn build_certified_key(&mut self) -> IoResult<CertifiedKey> {
        let cert = rustls_pemfile::certs(&mut self.cert.as_ref())
            .flat_map(|certs| certs.into_iter().collect::<Vec<CertificateDer<'static>>>())
            .collect::<Vec<_>>();

        let key = {
            let mut ec = rustls_pemfile::ec_private_keys(&mut self.key.as_ref())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| IoError::other("failed to parse tls private keys"))?;
            if !ec.is_empty() {
                PrivateKeyDer::Sec1(ec.remove(0))
            } else {
                let mut pkcs8 = rustls_pemfile::pkcs8_private_keys(&mut self.key.as_ref())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| IoError::other("failed to parse tls private keys"))?;
                if !pkcs8.is_empty() {
                    PrivateKeyDer::Pkcs8(pkcs8.remove(0))
                } else {
                    let mut rsa = rustls_pemfile::rsa_private_keys(&mut self.key.as_ref())
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| IoError::other("failed to parse tls private keys"))?;

                    if !rsa.is_empty() {
                        PrivateKeyDer::Pkcs1(rsa.remove(0))
                    } else {
                        return Err(IoError::other("failed to parse tls private keys"));
                    }
                }
            }
        };

        let key = any_supported_type(&key).map_err(|_| IoError::other("invalid private key"))?;

        Ok(CertifiedKey {
            cert,
            key,
            ocsp: if !self.ocsp_resp.is_empty() {
                Some(self.ocsp_resp.clone())
            } else {
                None
            },
        })
    }
}

/// Tls client authentication configuration.
#[derive(Clone, Debug)]
pub enum TlsClientAuth {
    /// No client auth.
    Off,
    /// Allow any anonymous or authenticated client.
    Optional(Vec<u8>),
    /// Allow any authenticated client.
    Required(Vec<u8>),
}

#[allow(clippy::vec_init_then_push)]
fn alpn_protocols() -> Vec<Vec<u8>> {
    #[allow(unused_mut)]
    let mut alpn_protocols = Vec::with_capacity(3);
    #[cfg(feature = "quinn")]
    alpn_protocols.push(b"h3".to_vec());
    #[cfg(feature = "http2")]
    alpn_protocols.push(b"h2".to_vec());
    #[cfg(feature = "http1")]
    alpn_protocols.push(b"http/1.1".to_vec());
    alpn_protocols
}

/// Builder to set the configuration for the Tls server.
#[derive(Clone, Debug)]
pub struct RustlsConfig {
    /// Fallback keycert.
    pub fallback: Option<Keycert>,
    /// Keycerts.
    pub keycerts: HashMap<String, Keycert>,
    /// Client auth.
    pub client_auth: TlsClientAuth,
    /// Protocols through ALPN (Application-Layer Protocol Negotiation).
    pub alpn_protocols: Vec<Vec<u8>>,
}


impl RustlsConfig {
    /// Create new `RustlsConfig`
    #[inline]
    pub fn new(fallback: impl Into<Option<Keycert>>) -> Self {
        RustlsConfig {
            fallback: fallback.into(),
            keycerts: HashMap::new(),
            client_auth: TlsClientAuth::Off,
            alpn_protocols: alpn_protocols(),
        }
    }

    /// Sets the trust anchor for optional Tls client authentication via file path.
    ///
    /// Anonymous and authenticated clients will be accepted. If no trust anchor is provided by any
    /// of the `client_auth_` methods, then client authentication is disabled by default.
    pub fn client_auth_optional_path(mut self, path: impl AsRef<Path>) -> IoResult<Self> {
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
    pub fn client_auth_required_path(mut self, path: impl AsRef<Path>) -> IoResult<Self> {
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

    /// Set specific protocols through ALPN (Application-Layer Protocol Negotiation).
    #[inline]
    pub fn alpn_protocols(mut self, alpn_protocols: impl Into<Vec<Vec<u8>>>) -> Self {
        self.alpn_protocols = alpn_protocols.into();
        self
    }

    /// ServerConfig
    pub(crate) fn build_server_config(mut self) -> IoResult<ServerConfig> {
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
            TlsClientAuth::Off => WebPkiClientVerifier::no_client_auth(),
            TlsClientAuth::Optional(trust_anchor) => {
                WebPkiClientVerifier::builder(read_trust_anchor(trust_anchor)?.into())
                    .allow_unauthenticated()
                    .build()
                    .map_err(|e| IoError::other(format!("failed to build server config: {}", e)))?
            }
            TlsClientAuth::Required(trust_anchor) => {
                WebPkiClientVerifier::builder(read_trust_anchor(trust_anchor)?.into())
                    .build()
                    .map_err(|e| IoError::other(format!("failed to build server config: {}", e)))?
            }
        };

        let mut config = ServerConfig::builder()
            .with_client_cert_verifier(client_auth)
            .with_cert_resolver(Arc::new(CertResolver {
                certified_keys,
                fallback,
            }));
        config.alpn_protocols = self.alpn_protocols;
        Ok(config)
    }

    cfg_feature! {
        #![feature = "quinn"]
        /// Build quinn server config.
        pub fn build_quinn_config(self) -> IoResult<crate::conn::quinn::ServerConfig> {
            self.try_into()
        }
    }
}

impl TryInto<ServerConfig> for RustlsConfig {
    type Error = IoError;

    fn try_into(self) -> IoResult<ServerConfig> {
        self.build_server_config()
    }
}

cfg_feature! {
    #![feature = "quinn"]
    impl TryInto<crate::conn::quinn::ServerConfig> for RustlsConfig {
        type Error = IoError;

        fn try_into(self) -> IoResult<crate::conn::quinn::ServerConfig> {
            let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(self.build_server_config()?).map_err(|_|IoError::other( "failed to build quinn server config"))?;
            Ok(crate::conn::quinn::ServerConfig::with_crypto(Arc::new(crypto)))
        }
    }
}

#[derive(Debug)]
pub(crate) struct CertResolver {
    fallback: Option<Arc<CertifiedKey>>,
    certified_keys: HashMap<String, Arc<CertifiedKey>>,
}

impl ResolvesServerCert for CertResolver {
    fn resolve(&self, client_hello: ClientHello) -> Option<Arc<CertifiedKey>> {
        client_hello
            .server_name()
            .and_then(|name| self.certified_keys.get(name).cloned())
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
