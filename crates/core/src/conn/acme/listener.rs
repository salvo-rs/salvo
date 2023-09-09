use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::rustls::server::ServerConfig;
use tokio_rustls::rustls::sign::{any_ecdsa_type, CertifiedKey};
use tokio_rustls::rustls::PrivateKey;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

use crate::conn::{Accepted, Acceptor, Holding, Listener};

use crate::http::uri::Scheme;
use crate::http::Version;
use crate::{async_trait, Router};

use super::config::{AcmeConfig, AcmeConfigBuilder};
use super::resolver::{ResolveServerCert, ACME_TLS_ALPN_NAME};
use super::{AcmeCache, AcmeClient, ChallengeType, Http01Handler, WELL_KNOWN_PATH};

cfg_feature! {
    #![feature = "quinn"]
    use crate::conn::quinn::QuinnAcceptor;
    use crate::conn::joined::JoinedAcceptor;
    use crate::conn::quinn::QuinnListener;
    use futures_util::stream::BoxStream;
}
/// A wrapper around an underlying listener which implements the ACME.
pub struct AcmeListener<T> {
    inner: T,
    config_builder: AcmeConfigBuilder,
    check_duration: Duration,
}

impl<T> AcmeListener<T> {
    /// Create `AcmeListener`
    #[inline]
    pub fn new(inner: T) -> AcmeListener<T> {
        Self {
            inner,
            config_builder: AcmeConfig::builder(),
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
    pub fn domains(self, domains: impl Into<Vec<String>>) -> Self {
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
    pub fn contacts(self, contacts: impl Into<Vec<String>>) -> Self {
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
                .insert(0, Router::with_path(format!("{}/<token>", WELL_KNOWN_PATH)).goal(handler));
        } else {
            panic!("`HTTP-01` challenge's key should not be none");
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

    cfg_feature! {
        #![feature = "quinn"]
        /// Enable Http3 using quinn.
        pub fn quinn<A>(self, local_addr: A) -> AcmeQuinnListener<T, A>
        where
            A: std::net::ToSocketAddrs + Send,
        {
            AcmeQuinnListener::new(self, local_addr)
        }
    }
}

#[async_trait]
impl<T> Listener for AcmeListener<T>
where
    T: Listener + Send,
    T::Acceptor: Send + 'static,
{
    type Acceptor = AcmeAcceptor<T::Acceptor>;

    async fn try_bind(mut self) -> IoResult<Self::Acceptor> {
        let Self {
            inner,
            config_builder,
            check_duration,
            ..
        } = self;
        let acme_config = config_builder.build()?;
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

        let mut server_config = ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_cert_resolver(cert_resolver.clone());

        server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        if acme_config.challenge_type == ChallengeType::TlsAlpn01 {
            server_config.alpn_protocols.push(ACME_TLS_ALPN_NAME.to_vec());
        }
        let server_config = Arc::new(server_config);

        let tls_acceptor = TlsAcceptor::from(server_config.clone());
        let inner = inner.try_bind().await?;
        let acceptor = AcmeAcceptor::new(acme_config, server_config, cert_resolver, inner, tls_acceptor, check_duration).await?;
        Ok(acceptor)
    }
}

cfg_feature! {
    #![feature = "quinn"]
    /// A wrapper around an underlying listener which implements the ACME and Quinn.
    pub struct AcmeQuinnListener<T, A> {
        acme: AcmeListener<T>,
        local_addr: A,
    }

    impl <T, A> AcmeQuinnListener<T, A>
    where
        A: std::net::ToSocketAddrs + Send,
    {
        pub(crate) fn new(acme: AcmeListener<T>, local_addr: A) -> Self {
            Self { acme, local_addr }
        }
    }

    #[async_trait]
    impl<T, A> Listener for AcmeQuinnListener<T, A>
    where
        T: Listener + Send,
        T::Acceptor: Send + Unpin + 'static,
        A: std::net::ToSocketAddrs + Send,
    {
        type Acceptor = JoinedAcceptor<AcmeAcceptor<T::Acceptor>, QuinnAcceptor<BoxStream<'static, crate::conn::quinn::ServerConfig>, crate::conn::quinn::ServerConfig, std::convert::Infallible>>;

        async fn try_bind(self) -> IoResult<Self::Acceptor> {
            let Self { acme, local_addr } = self;
            let a = acme.try_bind().await?;

            let mut crypto = a.server_config.as_ref().clone();
            crypto.alpn_protocols = vec![b"h3-29".to_vec(), b"h3-28".to_vec(), b"h3-27".to_vec(), b"h3".to_vec()];
            let config = crate::conn::quinn::ServerConfig::with_crypto(Arc::new(crypto));
            let b = QuinnListener::new(futures_util::stream::once(async {config}), local_addr).try_bind().await?;
            let holdings = a.holdings().iter().chain(b.holdings().iter()).cloned().collect();
            Ok(JoinedAcceptor::new(a, b, holdings))
        }
    }
}

/// AcmeAcceptor
pub struct AcmeAcceptor<T> {
    config: Arc<AcmeConfig>,
    server_config: Arc<ServerConfig>,
    inner: T,
    holdings: Vec<Holding>,
    tls_acceptor: tokio_rustls::TlsAcceptor,
}

impl<T> AcmeAcceptor<T>
where
    T: Acceptor + Send,
{
    pub(crate) async fn new(
        config: impl Into<Arc<AcmeConfig>> + Send,
        server_config: impl Into<Arc<ServerConfig>> + Send,
        cert_resolver: Arc<ResolveServerCert>,
        inner: T,
        tls_acceptor: TlsAcceptor,
        check_duration: Duration,
    ) -> IoResult<AcmeAcceptor<T>>
    where
        T: Send,
    {
        let holdings = inner
            .holdings()
            .iter()
            .map(|h| {
                let mut versions = h.http_versions.clone();
                #[cfg(feature = "http1")]
                if !versions.contains(&Version::HTTP_11) {
                    versions.push(Version::HTTP_11);
                }
                #[cfg(feature = "http2")]
                if !versions.contains(&Version::HTTP_2) {
                    versions.push(Version::HTTP_2);
                }
                Holding {
                    local_addr: h.local_addr.clone(),
                    http_versions: versions,
                    http_scheme: Scheme::HTTPS,
                }
            })
            .collect();

        let acceptor = AcmeAcceptor {
            config: config.into(),
            server_config: server_config.into(),
            inner,
            holdings,
            tls_acceptor,
        };
        let config = acceptor.config.clone();
        let weak_cert_resolver = Arc::downgrade(&cert_resolver);
        let mut client =
            AcmeClient::new(&config.directory_url, config.key_pair.clone(), config.contacts.clone()).await?;
        tokio::spawn(async move {
            while let Some(cert_resolver) = Weak::upgrade(&weak_cert_resolver) {
                if cert_resolver.will_expired(config.before_expired) {
                    if let Err(e) = super::issuer::issue_cert(&mut client, &config, &cert_resolver).await {
                        tracing::error!(error = ?e, "issue certificate failed");
                    }
                }
                tokio::time::sleep(check_duration).await;
            }
        });
        Ok(acceptor)
    }
    pub fn server_config(&self) -> Arc<ServerConfig> {
        self.server_config.clone()
    }
}
#[async_trait]
impl<T: Acceptor> Acceptor for AcmeAcceptor<T>
where
    T: Acceptor + Send + 'static,
    <T as Acceptor>::Conn: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Conn = TlsStream<T::Conn>;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(&mut self) -> IoResult<Accepted<Self::Conn>> {
        let Accepted {
            conn,
            local_addr,
            remote_addr,
            http_version,
            http_scheme,
        } = self.inner.accept().await?;
        let conn = self
            .tls_acceptor
            .accept(conn)
            .await
            .map_err(|e| IoError::new(ErrorKind::Other, e.to_string()))?;
        Ok(Accepted {
            conn,
            local_addr,
            remote_addr,
            http_version,
            http_scheme,
        })
    }
}
