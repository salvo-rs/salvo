use std::fmt::{self, Debug, Formatter};
use std::io::Result as IoResult;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use certon::crypto::KeyType;
use certon::handshake::CertResolver;
use certon::solvers::Solver;
use certon::storage::Storage;
use certon::{AcmeIssuer, FileStorage, OcspConfig, OnDemandConfig, ZeroSslIssuer};
use salvo_core::conn::tcp::{DynTcpAcceptor, TcpCoupler, ToDynTcpAcceptor};
use salvo_core::conn::{Accepted, Acceptor, HandshakeStream, Holding, Listener};
use salvo_core::fuse::ArcFusePolicy;
use salvo_core::http::Version;
use salvo_core::http::uri::Scheme;
use salvo_core::{Result as CoreResult, Router, cfg_feature};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::crypto::CryptoProvider;
use tokio_rustls::rustls::server::{ClientHello, ResolvesServerCert, ServerConfig};
use tokio_rustls::rustls::sign::CertifiedKey;
use tokio_rustls::server::TlsStream;

use super::config::{AcmeConfig, AcmeConfigBuilder};
use super::{ChallengeType, Http01Handler, WELL_KNOWN_PATH};

cfg_feature! {
    #![feature = "quinn"]
    use salvo_core::conn::quinn::QuinnAcceptor;
    use salvo_core::conn::JoinedAcceptor;
    use salvo_core::conn::quinn::QuinnListener;
}

/// ACME TLS-ALPN-01 protocol name.
const ACME_TLS_ALPN_NAME: &[u8] = b"acme-tls/1";

fn configure_contacts(
    builder: certon::acme_issuer::AcmeIssuerBuilder,
    contacts: &[String],
) -> certon::acme_issuer::AcmeIssuerBuilder {
    if contacts.is_empty() {
        return builder;
    }
    let contacts: Vec<_> = contacts
        .iter()
        .map(|contact| {
            let contact = contact.trim();
            if contact
                .get(..7)
                .is_some_and(|scheme| scheme.eq_ignore_ascii_case("mailto:"))
            {
                format!("mailto:{}", &contact[7..])
            } else {
                format!("mailto:{contact}")
            }
        })
        .collect();
    // Certon uses email for account identity, but accepts the complete ACME
    // contact URI list through its account customization callback.
    builder
        .email(contacts[0].strip_prefix("mailto:").unwrap_or(&contacts[0]))
        .new_account_func(Arc::new(move |account| account.contact = contacts.clone()))
}

struct AcmeMaintenance {
    manager: Arc<certon::CertManager>,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl Drop for AcmeMaintenance {
    fn drop(&mut self) {
        self.manager.cache.stop();
        for task in self
            .tasks
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
        {
            task.abort();
        }
    }
}

struct ManagedResolver {
    resolver: Arc<CertResolver>,
    // ServerConfig clones (including QUIC) share the same lifecycle owner.
    _maintenance: Arc<AcmeMaintenance>,
}

impl Debug for ManagedResolver {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ManagedResolver").finish_non_exhaustive()
    }
}

impl ResolvesServerCert for ManagedResolver {
    fn resolve(&self, hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        self.resolver.resolve(hello)
    }
}

impl AcmeMaintenance {
    fn start(&self, before_expired: Duration, check_duration: Duration) {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.push(certon::start_maintenance(&self.manager));
        tasks.push(tokio::spawn(maintain_absolute_window(
            Arc::downgrade(&self.manager),
            before_expired,
            check_duration,
        )));
    }
}

fn within_renewal_window(cert: &certon::Certificate, before_expired: Duration) -> bool {
    let expires: std::time::SystemTime = cert.not_after.into();
    let begins: std::time::SystemTime = cert.not_before.into();
    // A window longer than a short certificate's lifetime must not cause
    // perpetual immediate issuance after replacement.
    let window = before_expired.min(expires.duration_since(begins).unwrap_or_default() / 2);
    expires
        .duration_since(std::time::SystemTime::now())
        .unwrap_or_default()
        <= window
}

async fn maintain_absolute_window(
    manager: Weak<certon::CertManager>,
    before_expired: Duration,
    check_duration: Duration,
) {
    let check_duration = if check_duration.is_zero()
        || tokio::time::Instant::now()
            .checked_add(check_duration)
            .is_none()
    {
        Duration::from_secs(10 * 60)
    } else {
        check_duration
    };
    let mut interval = tokio::time::interval(check_duration);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        let Some(manager) = manager.upgrade() else {
            return;
        };
        // The absolute renewal window supplements upstream ARI and
        // emergency/revocation maintenance without retaining this owner.
        for cert in manager.cache.get_managed_certificates().await {
            if !within_renewal_window(&cert, before_expired) {
                continue;
            }
            let Some(domain) = cert.names.first() else {
                continue;
            };
            if let Err(error) = renew_in_absolute_window(&manager, domain, before_expired).await {
                tracing::warn!(domain, error = ?error, "absolute-window certificate renewal failed");
            }
        }
    }
}

async fn renew_in_absolute_window(
    manager: &certon::CertManager,
    domain: &str,
    before_expired: Duration,
) -> certon::Result<()> {
    // Serialize Salvo's forced-window checks across listeners sharing storage.
    // Refresh after taking the lock so a previous listener's replacement does
    // not result in another forced renewal from a stale local cache snapshot.
    let lock = certon::storage::acquire(
        manager.storage.clone(),
        &format!("salvo_absolute_renewal:{domain}"),
    )
    .await?;
    let cert = manager.cache_certificate(domain).await?;
    if within_renewal_window(&cert, before_expired) {
        manager.renew(domain, true).await?;
    }
    lock.release().await
}

/// Returns the [`CryptoProvider`] used to build the ACME `ServerConfig`.
///
/// Reuses the process level provider when the application installed one, otherwise builds one
/// from this crate's `aws-lc-rs` / `ring` features without installing it globally. Passing the
/// provider explicitly keeps rustls from panicking when feature unification makes both backends
/// available at once. `ring` wins when both are on, matching this crate's default.
pub(crate) fn default_crypto_provider() -> Arc<CryptoProvider> {
    if let Some(provider) = CryptoProvider::get_default() {
        return Arc::clone(provider);
    }

    #[cfg(any(feature = "ring", not(feature = "aws-lc-rs")))]
    {
        Arc::new(tokio_rustls::rustls::crypto::ring::default_provider())
    }
    #[cfg(all(not(feature = "ring"), feature = "aws-lc-rs"))]
    {
        Arc::new(tokio_rustls::rustls::crypto::aws_lc_rs::default_provider())
    }
}

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

    /// Applies a complete public ACME configuration, including custom solvers
    /// and renewal/account settings. It is validated when this listener binds.
    #[must_use]
    pub fn config(mut self, config: AcmeConfig) -> Self {
        self.config_builder = config;
        self
    }

    /// Sets the absolute time before certificate expiry to start renewal.
    /// The window is capped at half the certificate lifetime.
    /// Authority ARI, revocation and emergency renewal can run earlier.
    #[must_use]
    pub fn before_expired(mut self, duration: Duration) -> Self {
        self.config_builder = self.config_builder.before_expired(duration);
        self
    }

    /// Sets the interval between checks of the absolute renewal window.
    /// Defaults to ten minutes. Zero or an overflowing interval uses the default.
    #[must_use]
    pub fn check_duration(mut self, duration: Duration) -> Self {
        self.check_duration = duration;
        self
    }

    /// Sets the directory.
    ///
    /// Defaults to Let's Encrypt production.
    #[inline]
    #[must_use]
    pub fn directory(self, name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            config_builder: self.config_builder.directory(name, url),
            ..self
        }
    }

    /// Deprecated alias for [`Self::directory`].
    ///
    /// The `get_` prefix was misleading: this is a setter, not a getter.
    #[deprecated(since = "0.94.0", note = "use `directory` instead")]
    #[inline]
    #[must_use]
    pub fn get_directory(self, name: impl Into<String>, url: impl Into<String>) -> Self {
        self.directory(name, url)
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

    /// Create a handler for HTTP-01 challenge.
    ///
    /// Serve `router` on port 80 before awaiting this listener's bind. Initial
    /// certificate issuance waits for the CA to read that challenge route.
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
            // `AcmeConfigBuilder::http01_challenge()` always populates
            // `keys_for_http01`, so this branch is an internal invariant violation
            // rather than a user-triggerable error.
            unreachable!("`http01_challenge()` must populate `keys_for_http01`");
        }
        Self {
            config_builder,
            ..self
        }
    }

    /// Create a handler for TLS-ALPN-01 challenge.
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
    /// This is not a necessary option. Without a cache path or custom storage,
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
    /// `EcdsaP256` is the default and recommended key type for newly generated
    /// certificates. RSA variants are available for compatibility, but must be
    /// selected explicitly.
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
        /// Enable HTTP/3 using quinn.
        pub fn quinn<A>(self, local_addr: A) -> AcmeQuinnListener<T, A>
        where
            A: std::net::ToSocketAddrs + Send,
        {
            AcmeQuinnListener::new(self, local_addr)
        }
    }

    /// Build a certon CertManager from our AcmeConfig.
    async fn build_certon_manager(
        config: &AcmeConfig,
        cache: Arc<certon::CertCache>,
        cert_resolver: Arc<CertResolver>,
    ) -> CoreResult<certon::CertManager> {
        // Determine storage backend.
        let storage: Arc<dyn Storage> = if let Some(ref s) = config.storage {
            s.clone()
        } else if let Some(ref path) = config.cache_path {
            Arc::new(FileStorage::new(path))
        } else {
            Arc::new(super::memory_storage::MemoryStorage::default())
        };

        // Build the ACME issuer via builder pattern.
        let mut acme_builder = configure_contacts(
            AcmeIssuer::builder()
                .ca(&config.directory_url)
                .agreed(config.agree_to_tos)
                .storage(storage.clone()),
            &config.contacts,
        );

        // Configure challenge solver based on challenge type.
        match config.challenge_type {
            ChallengeType::Http01 => {
                let solver = config.http01_solver.clone().unwrap_or_else(|| {
                    Arc::new(super::solvers::TrackedSolver(Arc::new(
                        super::solvers::Http01Solver {
                            keys: config.keys_for_http01.clone().expect("HTTP-01 map exists"),
                        },
                    )))
                });
                acme_builder = acme_builder
                    .disable_tlsalpn_challenge(true)
                    .http01_solver(solver);
            }
            ChallengeType::TlsAlpn01 => {
                let solver = config.tls_alpn01_solver.clone().unwrap_or_else(|| {
                    Arc::new(super::solvers::TrackedSolver(Arc::new(
                        super::solvers::TlsAlpn01Solver {
                            resolver: cert_resolver,
                        },
                    )))
                });
                acme_builder = acme_builder
                    .disable_http_challenge(true)
                    .tlsalpn01_solver(solver);
            }
            ChallengeType::Dns01 => {
                acme_builder = acme_builder
                    .disable_http_challenge(true)
                    .disable_tlsalpn_challenge(true);
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
        issuers.push(Arc::new(super::solvers::AcmeIssuer(acme_issuer)));

        let mut certon_builder = certon::CertManager::builder()
            .storage(storage)
            .cache(cache)
            .issuers(issuers)
            // Absolute before_expired checks below own ordinary renewals.
            // Keep upstream ARI/emergency/revocation safeguards enabled.
            .renewal_window_ratio(f64::MIN_POSITIVE)
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
    ) -> CoreResult<(ServerConfig, Arc<AcmeMaintenance>)> {
        // Build the cert resolver backed by certon's cache.
        let cache = certon::CertCache::new(Default::default());
        let manager_slot: Arc<OnceLock<Weak<certon::CertManager>>> = Arc::new(OnceLock::new());
        let cert_resolver = if let Some(ref on_demand) = config.on_demand {
            let configured = if on_demand.obtain_func.is_some() {
                on_demand.clone()
            } else {
                let slot = manager_slot.clone();
                Arc::new(OnDemandConfig {
                    decision_func: on_demand.decision_func.clone(),
                    host_allowlist: on_demand.host_allowlist.clone(),
                    rate_limit: on_demand.rate_limit.clone(),
                    obtain_func: Some(Arc::new(move |domain| {
                        let manager = slot.get().and_then(Weak::upgrade);
                        Box::pin(async move {
                            let manager = manager.ok_or_else(|| {
                                certon::Error::Config("ACME listener has stopped".into())
                            })?;
                            manager.obtain(&domain).await?;
                            manager.cache_certificate(&domain).await?;
                            Ok(())
                        })
                    })),
                })
            };
            CertResolver::with_on_demand(cache.clone(), configured)
        } else {
            CertResolver::new(cache.clone())
        };
        let cert_resolver = Arc::new(cert_resolver);
        let certon_manager =
            Arc::new(Self::build_certon_manager(config, cache, cert_resolver.clone()).await?);
        let _ = manager_slot.set(Arc::downgrade(&certon_manager));
        let maintenance = Arc::new(AcmeMaintenance {
            manager: certon_manager,
            tasks: Mutex::new(Vec::new()),
        });

        let mut server_config = ServerConfig::builder_with_provider(default_crypto_provider())
            .with_safe_default_protocol_versions()
            .map_err(salvo_core::Error::other)?
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(ManagedResolver {
                resolver: cert_resolver,
                _maintenance: maintenance.clone(),
            }));

        server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        if config.challenge_type == ChallengeType::TlsAlpn01 {
            server_config
                .alpn_protocols
                .push(ACME_TLS_ALPN_NAME.to_vec());
        }

        Ok((server_config, maintenance))
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
            check_duration,
        } = self;

        let acme_config = config_builder.build()?;
        let mut inner = inner.try_bind().await?;
        let (server_config, maintenance) = Self::build_server_config(&acme_config).await?;
        let certon_manager = &maintenance.manager;
        let server_config = Arc::new(server_config);
        let tls_acceptor = TlsAcceptor::from(server_config.clone());

        // Certificate issuance remains synchronous. While the CA validates a
        // TLS-ALPN-01 challenge, complete handshakes on the bound listener using
        // the same resolver that will serve renewals after startup.
        {
            let mut handshakes = tokio::task::JoinSet::new();
            let mut accept_retry_at = None;
            let management = certon_manager.manage(&acme_config.domains);
            tokio::pin!(management);
            loop {
                tokio::select! {
                    result = &mut management => {
                        if let Err(error) = result {
                            tracing::warn!(error = ?error, "initial certificate management failed");
                        }
                        break;
                    }
                    accepted = async {
                        if let Some(deadline) = accept_retry_at {
                            tokio::time::sleep_until(deadline).await;
                        }
                        inner.accept(None).await
                    }, if acme_config.challenge_type == ChallengeType::TlsAlpn01 && handshakes.len() < 64 => {
                        let accepted = match accepted {
                            Ok(accepted) => accepted,
                            Err(error) => {
                                tracing::error!(error = ?error, "accept connection failed during initial certificate management");
                                // Match the server's accept-error backoff. Keep the
                                // delay in this select branch so certificate loading
                                // or issuance can still finish while accept waits.
                                accept_retry_at = Some(tokio::time::Instant::now() + Duration::from_millis(10));
                                continue;
                            }
                        };
                        accept_retry_at = None;
                        let tls_acceptor = tls_acceptor.clone();
                        handshakes.spawn(async move {
                            let _ = tokio::time::timeout(Duration::from_secs(10), tls_acceptor.accept(accepted.stream)).await;
                        });
                    }
                    Some(_) = handshakes.join_next(), if !handshakes.is_empty() => {}
                }
            }
            handshakes.abort_all();
            while handshakes.join_next().await.is_some() {}
        }
        // Certon's obtain path persists new certificates, but unlike its load
        // path does not put them in the resolver's cache.
        for domain in &acme_config.domains {
            if let Err(error) = certon_manager.cache_certificate(domain).await {
                tracing::warn!(domain, error = ?error, "certificate unavailable after initial management");
            }
        }

        maintenance.start(acme_config.before_expired, check_duration);

        let acceptor = AcmeAcceptor::new(acme_config, server_config, inner, tls_acceptor);
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
        type Acceptor = JoinedAcceptor<AcmeAcceptor<T::Acceptor>, QuinnAcceptor>;

        async fn try_bind(self) -> CoreResult<Self::Acceptor> {
            let Self { acme, local_addr } = self;
            let a = acme.try_bind().await?;

            let mut crypto = a.server_config.as_ref().clone();
            crypto.alpn_protocols = vec![b"h3-29".to_vec(), b"h3-28".to_vec(), b"h3-27".to_vec(), b"h3".to_vec()];
            let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(crypto).map_err(salvo_core::Error::other)?;
            let config = quinn::ServerConfig::with_crypto(Arc::new(crypto));
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
    tls_acceptor: TlsAcceptor,
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
        fuse_policy: Option<ArcFusePolicy>,
    ) -> IoResult<Accepted<Self::Coupler, Self::Stream>> {
        let Accepted {
            coupler: _,
            stream,
            fuse_config,
            conn_ctrl,
            local_addr,
            remote_addr,
            ..
        } = self.inner.accept(fuse_policy).await?;
        Ok(Accepted {
            coupler: TcpCoupler::new(),
            stream: salvo_core::conn::rustls::accept_tls(
                self.tls_acceptor.accept(stream),
                fuse_config,
                conn_ctrl.clone(),
            ),
            fuse_config,
            conn_ctrl,
            local_addr,
            remote_addr,
            http_scheme: Scheme::HTTPS,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_contacts_are_separate_normalized_uris() {
        let issuer = configure_contacts(
            AcmeIssuer::builder().storage(Arc::new(
                super::super::memory_storage::MemoryStorage::default(),
            )),
            &[
                " first@example.com ".into(),
                "MAILTO:second@example.com".into(),
                "mailto:third@example.com".into(),
            ],
        )
        .build();
        assert_eq!(issuer.email, "first@example.com");
        let mut account = certon::account::new_account(&issuer.email, KeyType::EcdsaP256).unwrap();
        issuer.new_account_func.unwrap()(&mut account);
        assert_eq!(
            account.contact,
            [
                "mailto:first@example.com",
                "mailto:second@example.com",
                "mailto:third@example.com"
            ]
        );
    }
}
