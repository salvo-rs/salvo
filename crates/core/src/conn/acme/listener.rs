use std::collections::HashSet;
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;
use std::net::ToSocketAddrs;

use tokio_rustls::rustls::server::ServerConfig;
use tokio_rustls::rustls::sign::{any_ecdsa_type, CertifiedKey};
use tokio_rustls::rustls::PrivateKey;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

use crate::conn::{Accepted, Acceptor, TlsConnStream, Listener, SocketAddr, TcpListener};
use crate::{async_trait, Router};

use super::config::{AcmeConfig, AcmeConfigBuilder};
use super::resolver::{ResolveServerCert, ACME_TLS_ALPN_NAME};
use super::{AcmeCache, AcmeClient, ChallengeType, Http01Handler, WELL_KNOWN_PATH};

/// A wrapper around an underlying listener which implements the ACME.
pub struct AcmeListener<T> {
    inner: T,
    tls_acceptor: TlsAcceptor,
}

impl<T> AcmeListener<T> {
    /// Create `Builder`
    pub fn builder() -> Builder {
        Builder::new()
    }
}
/// Builder
pub struct Builder {
    config_builder: AcmeConfigBuilder,
    check_duration: Duration,
}
impl Builder {
    #[inline]
    fn new() -> Self {
        let config_builder = AcmeConfig::builder();
        Self {
            config_builder,
            check_duration: Duration::from_secs(10 * 60),
        }
    }

    /// Sets the directory.
    ///
    /// Defaults to lets encrypt.
    #[inline]
    pub fn get_directory(self, name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            config_builder: self.config_builder.directory(name, url),
            ..self
        }
    }

    /// Sets domains.
    #[inline]
    pub fn domains(self, domains: impl Into<HashSet<String>>) -> Self {
        Self {
            config_builder: self.config_builder.domains(domains),
            ..self
        }
    }
    /// Add a domain.
    #[inline]
    pub fn add_domain(self, domain: impl Into<String>) -> Self {
        Self {
            config_builder: self.config_builder.add_domain(domain),
            ..self
        }
    }

    /// Add contact emails for the ACME account.
    #[inline]
    pub fn contacts(self, contacts: impl Into<HashSet<String>>) -> Self {
        Self {
            config_builder: self.config_builder.contacts(contacts.into()),
            ..self
        }
    }
    /// Add a contact email for the ACME account.
    #[inline]
    pub fn add_contact(self, contact: impl Into<String>) -> Self {
        Self {
            config_builder: self.config_builder.add_contact(contact.into()),
            ..self
        }
    }

    /// Create an handler for HTTP-01 challenge
    #[inline]
    pub fn http01_challege(self, router: &mut Router) -> Self {
        let config_builder = self.config_builder.http01_challege();
        if let Some(keys_for_http01) = &config_builder.keys_for_http01 {
            let handler = Http01Handler {
                keys: keys_for_http01.clone(),
            };
            router
                .routers
                .push(Router::with_path(format!("{}/<token>", WELL_KNOWN_PATH)).handle(handler));
        } else {
            panic!("`HTTP-01` challage's key should not none");
        }
        Self { config_builder, ..self }
    }
    /// Create an handler for HTTP-01 challenge
    #[inline]
    pub fn tls_alpn01_challege(self) -> Self {
        Self {
            config_builder: self.config_builder.tls_alpn01_challege(),
            ..self
        }
    }

    /// Sets the cache path for caching certificates.
    ///
    /// This is not a necessary option. If you do not configure the cache path,
    /// the obtained certificate will be stored in memory and will need to be
    /// obtained again when the server is restarted next time.
    #[inline]
    pub fn cache_path(self, path: impl Into<PathBuf>) -> Self {
        Self {
            config_builder: self.config_builder.cache_path(path),
            ..self
        }
    }

    #[inline]
    pub async fn build<T>(self, inner: T) -> IoResult<AcmeListener<T>> {
        let Self {
            config_builder,
            check_duration,
        } = self;
        let acme_config = config_builder.build()?;

        let mut client = AcmeClient::try_new(
            &acme_config.directory_url,
            acme_config.key_pair.clone(),
            acme_config.contacts.clone(),
        )
        .await?;

        let mut cached_key = None;
        let mut cached_cert = None;
        if let Some(cache_path) = &acme_config.cache_path {
            let key_data = cache_path
                .read_key(&acme_config.directory_name, &acme_config.domains)
                .await?;
            if let Some(key_data) = key_data {
                tracing::debug!("load private key from cache");
                match rustls_pemfile::pkcs8_private_keys(&mut key_data.as_slice()) {
                    Ok(key) => cached_key = key.into_iter().next(),
                    Err(e) => {
                        tracing::warn!(error = ?e, "parse cached private key failed")
                    }
                };
            }
            let cert_data = cache_path
                .read_cert(&acme_config.directory_name, &acme_config.domains)
                .await?;
            if let Some(cert_data) = cert_data {
                tracing::debug!("load certificate from cache");
                match rustls_pemfile::certs(&mut cert_data.as_slice()) {
                    Ok(cert) => cached_cert = Some(cert),
                    Err(e) => {
                        tracing::warn!(error = ?e, "parse cached tls certificates failed")
                    }
                };
            }
        };

        let cert_resolver = Arc::new(ResolveServerCert::default());
        if let (Some(cached_cert), Some(cached_key)) = (cached_cert, cached_key) {
            let certs = cached_cert
                .into_iter()
                .map(tokio_rustls::rustls::Certificate)
                .collect::<Vec<_>>();
            tracing::debug!("using cached tls certificates");
            *cert_resolver.cert.write() = Some(Arc::new(CertifiedKey::new(
                certs,
                any_ecdsa_type(&PrivateKey(cached_key)).unwrap(),
            )));
        }

        let weak_cert_resolver = Arc::downgrade(&cert_resolver);
        let mut server_config = ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_cert_resolver(cert_resolver);

        server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        if acme_config.challenge_type == ChallengeType::TlsAlpn01 {
            server_config.alpn_protocols.push(ACME_TLS_ALPN_NAME.to_vec());
        }

        let tls_acceptor = TlsAcceptor::from(Arc::new(server_config));
        let listener = AcmeListener { inner, tls_acceptor };

        tokio::spawn(async move {
            while let Some(cert_resolver) = Weak::upgrade(&weak_cert_resolver) {
                if cert_resolver.will_expired(acme_config.before_expired) {
                    if let Err(e) = super::issuer::issue_cert(&mut client, &acme_config, &cert_resolver).await {
                        tracing::error!(error = ?e, "issue certificate failed");
                    }
                }
                tokio::time::sleep(check_duration).await;
            }
        });
        Ok(listener)
    }

    /// Consumes this builder and returns a [`AcmeListener`] object.
    #[inline]
    pub async fn bind(self, addr: impl ToSocketAddrs) -> AcmeListener<TcpListener> {
        self.try_bind(addr).await.unwrap()
    }
    /// Consumes this builder and returns a [`Result<AcmeListener, std::IoError>`] object.
    pub async fn try_bind(self, addr: impl ToSocketAddrs) -> IoResult<AcmeListener<TcpListener>> {
        let inner = TcpListener::try_bind(addr).await?;
        self.build(inner).await
    }
}

impl<T: Acceptor> Listener for AcmeListener<T> {}

#[async_trait]
impl<T: Acceptor> Acceptor for AcmeListener<T> {
    type Conn = TlsConnStream<TlsStream<T::Conn>>;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(&mut self) -> Result<Accepted<Self::Conn>, IoError> {
        let Accepted {
            mut stream,
            local_addr,
            remote_addr,
        } = self
            .inner
            .accept()
            .await
            .map_err(|e| IoError::new(ErrorKind::Other, format!("inner accept error: {}", e)))?;
        let stream = TlsConnStream::new(self.tls_acceptor.accept(stream));
        return Ok(Accepted {
            stream,
            local_addr,
            remote_addr,
        });
    }
}
