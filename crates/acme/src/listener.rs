use std::fmt::{self, Debug, Formatter};
use std::io::Result as IoResult;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use certon::crypto::KeyType;
use certon::handshake::CertResolver;
use certon::solvers::Solver;
use certon::storage::Storage;
use certon::{AcmeIssuer, FileStorage, OcspConfig, OnDemandConfig, ZeroSslIssuer};
use salvo_core::conn::tcp::{DynTcpAcceptor, TcpCoupler, ToDynTcpAcceptor};
use salvo_core::conn::{Accepted, Acceptor, HandshakeStream, Holding, Listener};
use salvo_core::fuse::ArcFuseFactory;
use salvo_core::http::Version;
use salvo_core::http::uri::Scheme;
use salvo_core::{Result as CoreResult, Router};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::server::ServerConfig;
use tokio_rustls::server::TlsStream;

use super::config::{AcmeConfig, AcmeConfigBuilder};
use super::{ChallengeType, Http01Handler, WELL_KNOWN_PATH};

cfg_feature! {
    #![feature = "quinn"]
    use salvo_core::conn::quinn::QuinnAcceptor;
    use salvo_core::conn::JoinedAcceptor;
    use salvo_core::conn::quinn::QuinnListener;
    use futures_util::stream::BoxStream;
}

/// ACME TLS-ALPN-01 protocol name.
const ACME_TLS_ALPN_NAME: &[u8] = b"acme-tls/1";

/// A wrapper around an underlying listener which implements ACME.
pub struct AcmeListenerBuilder<T> {
    inner: T,
    config_builder: AcmeConfigBuilder,
    check_duration: Duration,
}

impl<T> Debug for AcmeListenerBuilder<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("AcmeListenerBuilder")
            .field("inner", &self.inner)
            .field("config_builder", &self.config_builder)
            .field("check_duration", &self.check_duration)
            .finish()
    }
}

impl<T> AcmeListenerBuilder<T> {
    /// Create `AcmeListenerBuilder`.
    #[inline]
    #[must_use]
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            config_builder: AcmeConfig::builder(),
            check_duration: Duration::from_secs(10 * 60),
        }
    }

    /// Sets the directory.
    ///
    /// Defaults to Let's Encrypt production.
    #[inline]
    #[must_use]
    pub fn get_directory(self, name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            config_builder: self.config_builder.directory(name, url),
            ..self
        }
    }

    /// Sets domains.
    #[inline]
    #[must_use]
    pub fn domains(self, domains: impl Into<Vec<String>>) -> Self {
        Self {
            config_builder: self.config_builder.domains(domains),
            ..self
        }
    }

    /// Add a domain.
    #[inline]
    #[must_use]
    pub fn add_domain(self, domain: impl Into<String>) -> Self {
        Self {
            config_builder: self.config_builder.add_domain(domain),
            ..self
        }
    }

    /// Add contact emails for the ACME account.
    #[inline]
    #[must_use]
    pub fn contacts(self, contacts: impl Into<Vec<String>>) -> Self {
        Self {
            config_builder: self.config_builder.contacts(contacts.into()),
            ..self
        }
    }

    /// Add a contact email for the ACME account.
    #[inline]
    #[must_use]
    pub fn add_contact(self, contact: impl Into<String>) -> Self {
        Self {
            config_builder: self.config_builder.add_contact(contact.into()),
            ..self
        }
    }

    /// Create an handler for HTTP-01 challenge.
    #[must_use]
    pub fn http01_challenge(self, router: &mut Router) -> Self {
        let config_builder = self.config_builder.http01_challenge();
        if let Some(keys_for_http01) = &config_builder.keys_for_http01 {
            let handler = Http01Handler {
                keys: keys_for_http01.clone(),
            };
            router.routers.insert(
                0,
                Router::with_path(format!("{WELL_KNOWN_PATH}/{{token}}")).goal(handler),
            );
        } else {
            panic!("`HTTP-01` challenge's key should not be none");
        }
        Self {
            config_builder,
            ..self
        }
    }

    /// Create an handler for TLS-ALPN-01 challenge.
    #[inline]
    #[must_use]
    pub fn tls_alpn01_challenge(self) -> Self {
        Self {
            config_builder: self.config_builder.tls_alpn01_challenge(),
            ..self
        }
    }

    /// Configure DNS-01 challenge with a custom solver.
    #[inline]
    #[must_use]
    pub fn dns01_challenge(self, solver: Arc<dyn Solver>) -> Self {
        Self {
            config_builder: self.config_builder.dns01_challenge(solver),
            ..self
        }
    }

    /// Sets the cache path for caching certificates.
    ///
    /// This is not a necessary option. If you do not configure the cache path,
    /// the obtained certificate will be stored in memory and will need to be
    /// obtained again when the server is restarted next time.
    #[inline]
    #[must_use]
    pub fn cache_path(self, path: impl Into<PathBuf>) -> Self {
        Self {
            config_builder: self.config_builder.cache_path(path),
            ..self
        }
    }

    /// Sets the key type for certificate private keys.
    ///
    /// Available types: `EcdsaP256` (default), `EcdsaP384`, `EcdsaP521`,
    /// `Rsa2048`, `Rsa4096`, `Rsa8192`, `Ed25519`.
    #[inline]
    #[must_use]
    pub fn key_type(self, key_type: KeyType) -> Self {
        Self {
            config_builder: self.config_builder.key_type(key_type),
            ..self
        }
    }

    /// Sets the OCSP stapling configuration.
    #[inline]
    #[must_use]
    pub fn ocsp(self, ocsp: OcspConfig) -> Self {
        Self {
            config_builder: self.config_builder.ocsp(ocsp),
            ..self
        }
    }

    /// Configures on-demand TLS.
    #[inline]
    #[must_use]
    pub fn on_demand(self, on_demand: Arc<OnDemandConfig>) -> Self {
        Self {
            config_builder: self.config_builder.on_demand(on_demand),
            ..self
        }
    }

    /// Configures ZeroSSL as an additional issuer.
    #[inline]
    #[must_use]
    pub fn zerossl_api_key(self, api_key: impl Into<String>) -> Self {
        Self {
            config_builder: self.config_builder.zerossl_api_key(api_key),
            ..self
        }
    }

    /// Adds a custom certificate issuer.
    #[must_use]
    pub fn add_issuer(self, issuer: Arc<dyn certon::CertIssuer>) -> Self {
        Self {
            config_builder: self.config_builder.add_issuer(issuer),
            ..self
        }
    }

    /// Sets a custom persistent storage backend.
    #[inline]
    #[must_use]
    pub fn storage(self, storage: Arc<dyn Storage>) -> Self {
        Self {
            config_builder: self.config_builder.storage(storage),
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

    /// Build a certon Config from our AcmeConfig.
    async fn build_certon_config(config: &AcmeConfig) -> CoreResult<certon::Config> {
        // Determine storage backend.
        let storage: Arc<dyn Storage> = if let Some(ref s) = config.storage {
            s.clone()
        } else if let Some(ref path) = config.cache_path {
            Arc::new(FileStorage::new(path))
        } else {
            Arc::new(FileStorage::default())
        };

        // Build the ACME issuer via builder pattern.
        let mut acme_builder = AcmeIssuer::builder()
            .ca(&config.directory_url)
            .agreed(config.agree_to_tos)
            .storage(storage.clone());

        // Configure contacts.
        if !config.contacts.is_empty() {
            acme_builder = acme_builder.email(config.contacts.join(","));
        }

        // Configure challenge solver based on challenge type.
        match config.challenge_type {
            ChallengeType::Http01 => {
                if let Some(ref solver) = config.http01_solver {
                    acme_builder = acme_builder.http01_solver(solver.clone());
                }
                // If no custom solver, the Http01Handler serves challenges
                // through salvo's router — certon's global active_challenges
                // map is used as fallback.
            }
            ChallengeType::TlsAlpn01 => {
                if let Some(ref solver) = config.tls_alpn01_solver {
                    acme_builder = acme_builder.tlsalpn01_solver(solver.clone());
                }
            }
            ChallengeType::Dns01 => {
                if let Some(ref solver) = config.dns01_solver {
                    acme_builder = acme_builder.dns01_solver(solver.clone());
                }
            }
        }

        let acme_issuer = acme_builder.build();

        let mut issuers: Vec<Arc<dyn certon::CertIssuer>> = Vec::new();

        // Add custom issuers first.
        if let Some(ref custom_issuers) = config.issuers {
            issuers.extend(custom_issuers.iter().cloned());
        }

        // Add ZeroSSL issuer if configured.
        if let Some(ref api_key) = config.zerossl_api_key {
            match ZeroSslIssuer::builder()
                .api_key(api_key)
                .storage(storage.clone())
                .build()
                .await
            {
                Ok(zerossl) => {
                    issuers.push(Arc::new(zerossl));
                }
                Err(e) => {
                    tracing::warn!(error = ?e, "failed to initialize ZeroSSL issuer; skipping");
                }
            }
        }

        // Add the default ACME issuer.
        issuers.push(Arc::new(acme_issuer));

        let mut certon_builder = certon::Config::builder()
            .storage(storage)
            .issuers(issuers)
            .key_type(config.key_type)
            .ocsp(config.ocsp.clone());

        if let Some(ref on_demand) = config.on_demand {
            certon_builder = certon_builder.on_demand(on_demand.clone());
        }

        Ok(certon_builder.build())
    }

    /// Build the rustls ServerConfig backed by certon's CertResolver.
    async fn build_server_config(
        config: &AcmeConfig,
    ) -> CoreResult<(ServerConfig, Arc<CertResolver>, certon::Config)> {
        let certon_config: certon::Config = Self::build_certon_config(config).await?;

        // Attempt to load/obtain certificates for configured domains.
        if let Err(e) = certon_config.manage_sync(&config.domains).await {
            tracing::warn!(error = ?e, "initial certificate management failed; will retry in background");
        }

        // Build the cert resolver backed by certon's cache.
        let cert_resolver = if let Some(ref on_demand) = config.on_demand {
            CertResolver::with_on_demand(certon_config.cache.clone(), on_demand.clone())
        } else {
            CertResolver::new(certon_config.cache.clone())
        };
        let cert_resolver = Arc::new(cert_resolver);

        let mut server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(cert_resolver.clone());

        server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        if config.challenge_type == ChallengeType::TlsAlpn01 {
            server_config
                .alpn_protocols
                .push(ACME_TLS_ALPN_NAME.to_vec());
        }

        Ok((server_config, cert_resolver, certon_config))
    }
}

impl<T> Listener for AcmeListenerBuilder<T>
where
    T: Listener + Send + 'static,
    T::Acceptor: Send + 'static,
    <T::Acceptor as Acceptor>::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Acceptor = AcmeAcceptor<T::Acceptor>;

    async fn try_bind(self) -> CoreResult<Self::Acceptor> {
        let Self {
            inner,
            config_builder,
            ..
        } = self;

        let acme_config = config_builder.build()?;
        let (server_config, _cert_resolver, certon_config) =
            Self::build_server_config(&acme_config).await?;
        let server_config = Arc::new(server_config);
        let tls_acceptor = TlsAcceptor::from(server_config.clone());
        let inner = inner.try_bind().await?;

        // Start certon's background maintenance for renewal + OCSP.
        let _maintenance_handle = certon::start_maintenance(&certon_config);

        let acceptor = AcmeAcceptor::new(
            acme_config,
            server_config,
            inner,
            tls_acceptor,
        );
        Ok(acceptor)
    }
}

cfg_feature! {
    #![feature = "quinn"]
    /// A wrapper around an underlying listener which implements ACME and Quinn.
    pub struct AcmeQuinnListener<T, A> {
        acme: AcmeListenerBuilder<T>,
        local_addr: A,
    }

    impl<T: Debug, A: Debug> Debug for AcmeQuinnListener<T, A> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.debug_struct("AcmeQuinnListener")
                .field("acme", &self.acme)
                .field("local_addr", &self.local_addr)
                .finish()
        }
    }

    impl<T, A> AcmeQuinnListener<T, A>
    where
        A: std::net::ToSocketAddrs + Send,
    {
        /// Create `AcmeQuinnListener`.
        pub fn new(acme: AcmeListenerBuilder<T>, local_addr: A) -> Self {
            Self { acme, local_addr }
        }
    }

    impl<T, A> Listener for AcmeQuinnListener<T, A>
    where
        T: Listener + Send + 'static,
        T::Acceptor: Send + Unpin + 'static,
        <T::Acceptor as Acceptor>::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
        A: std::net::ToSocketAddrs + Send + 'static,
    {
        type Acceptor = JoinedAcceptor<AcmeAcceptor<T::Acceptor>, QuinnAcceptor<BoxStream<'static, salvo_core::conn::quinn::ServerConfig>, salvo_core::conn::quinn::ServerConfig, std::convert::Infallible>>;

        async fn try_bind(self) -> CoreResult<Self::Acceptor> {
            let Self { acme, local_addr } = self;
            let a = acme.try_bind().await?;

            let mut crypto = a.server_config.as_ref().clone();
            crypto.alpn_protocols = vec![b"h3-29".to_vec(), b"h3-28".to_vec(), b"h3-27".to_vec(), b"h3".to_vec()];
            let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(crypto).map_err(salvo_core::Error::other)?;
            let config = salvo_core::conn::quinn::ServerConfig::with_crypto(Arc::new(crypto));
            let b = QuinnListener::new(config, local_addr).try_bind().await?;
            Ok(JoinedAcceptor::new(a, b))
        }
    }
}

/// Acceptor for ACME.
pub struct AcmeAcceptor<T> {
    pub(crate) server_config: Arc<ServerConfig>,
    inner: T,
    holdings: Vec<Holding>,
    tls_acceptor: tokio_rustls::TlsAcceptor,
}
impl<T> Debug for AcmeAcceptor<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("AcmeAcceptor")
            .field("server_config", &self.server_config)
            .field("holdings", &self.holdings)
            .finish()
    }
}

impl<T> AcmeAcceptor<T>
where
    T: Acceptor + Send + 'static,
    T::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    pub(crate) fn new(
        _config: AcmeConfig,
        server_config: Arc<ServerConfig>,
        inner: T,
        tls_acceptor: TlsAcceptor,
    ) -> Self {
        let holdings = inner
            .holdings()
            .iter()
            .map(|h| {
                let mut versions = h.http_versions.clone();
                if !versions.contains(&Version::HTTP_11) {
                    versions.push(Version::HTTP_11);
                }
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

        Self {
            server_config,
            inner,
            holdings,
            tls_acceptor,
        }
    }

    /// Returns the config of this acceptor.
    pub fn server_config(&self) -> Arc<ServerConfig> {
        self.server_config.clone()
    }

    /// Convert this `AcmeAcceptor` into a boxed `DynTcpAcceptor`.
    pub fn into_boxed(self) -> Box<dyn DynTcpAcceptor> {
        Box::new(ToDynTcpAcceptor(self))
    }
}

impl<T> Acceptor for AcmeAcceptor<T>
where
    T: Acceptor + Send + 'static,
    <T as Acceptor>::Stream: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Coupler = TcpCoupler<Self::Stream>;
    type Stream = HandshakeStream<TlsStream<T::Stream>>;

    #[inline]
    fn holdings(&self) -> &[Holding] {
        &self.holdings
    }

    #[inline]
    async fn accept(
        &mut self,
        fuse_factory: Option<ArcFuseFactory>,
    ) -> IoResult<Accepted<Self::Coupler, Self::Stream>> {
        let Accepted {
            coupler: _,
            stream,
            fusewire,
            local_addr,
            remote_addr,
            ..
        } = self.inner.accept(fuse_factory).await?;
        Ok(Accepted {
            coupler: TcpCoupler::new(),
            stream: HandshakeStream::new(self.tls_acceptor.accept(stream), fusewire.clone()),
            fusewire,
            local_addr,
            remote_addr,
            http_scheme: Scheme::HTTPS,
        })
    }
}
