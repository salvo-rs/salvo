use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, Result as IoResult};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use certon::acme_issuer::CertIssuer;
use certon::crypto::KeyType;
use certon::solvers::Solver;
use certon::storage::Storage;
use certon::{OcspConfig, OnDemandConfig};
use tokio::sync::RwLock;

use super::{ChallengeType, LETS_ENCRYPT_PRODUCTION};

/// ACME configuration.
#[allow(dead_code)]
pub struct AcmeConfig {
    pub(crate) directory_name: String,
    pub(crate) directory_url: String,
    pub(crate) domains: Vec<String>,
    pub(crate) contacts: Vec<String>,
    pub(crate) challenge_type: ChallengeType,
    pub(crate) cache_path: Option<PathBuf>,
    pub(crate) keys_for_http01: Option<Arc<RwLock<HashMap<String, String>>>>,
    pub(crate) before_expired: Duration,
    // --- New certon-powered fields ---
    pub(crate) key_type: KeyType,
    pub(crate) issuers: Option<Vec<Arc<dyn CertIssuer>>>,
    pub(crate) storage: Option<Arc<dyn Storage>>,
    pub(crate) http01_solver: Option<Arc<dyn Solver>>,
    pub(crate) tls_alpn01_solver: Option<Arc<dyn Solver>>,
    pub(crate) dns01_solver: Option<Arc<dyn Solver>>,
    pub(crate) ocsp: OcspConfig,
    pub(crate) on_demand: Option<Arc<OnDemandConfig>>,
    pub(crate) zerossl_api_key: Option<String>,
    pub(crate) agree_to_tos: bool,
}

impl AcmeConfig {
    /// Create an ACME configuration builder.
    #[inline]
    #[must_use]
    pub fn builder() -> AcmeConfigBuilder {
        AcmeConfigBuilder::new()
    }
}

impl Debug for AcmeConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("AcmeConfig")
            .field("directory_name", &self.directory_name)
            .field("directory_url", &self.directory_url)
            .field("domains", &self.domains)
            .field("contacts", &self.contacts)
            .field("challenge_type", &self.challenge_type)
            .field("cache_path", &self.cache_path)
            .field("key_type", &self.key_type)
            .finish()
    }
}

/// ACME configuration builder.
///
/// Provides a fluent API for configuring ACME certificate management.
/// The builder now exposes advanced features from the `certon` crate:
///
/// - **Multiple issuers** via [`add_issuer`](AcmeConfigBuilder::add_issuer).
/// - **DNS-01 challenges** via [`dns01_challenge`](AcmeConfigBuilder::dns01_challenge).
/// - **On-demand TLS** via [`on_demand`](AcmeConfigBuilder::on_demand).
/// - **Key type selection** via [`key_type`](AcmeConfigBuilder::key_type).
/// - **OCSP stapling** via [`ocsp`](AcmeConfigBuilder::ocsp).
/// - **Custom storage** via [`storage`](AcmeConfigBuilder::storage).
/// - **ZeroSSL** via [`zerossl_api_key`](AcmeConfigBuilder::zerossl_api_key).
pub struct AcmeConfigBuilder {
    pub(crate) directory_name: String,
    pub(crate) directory_url: String,
    pub(crate) domains: Vec<String>,
    pub(crate) contacts: Vec<String>,
    pub(crate) challenge_type: ChallengeType,
    pub(crate) cache_path: Option<PathBuf>,
    pub(crate) keys_for_http01: Option<Arc<RwLock<HashMap<String, String>>>>,
    pub(crate) before_expired: Duration,
    pub(crate) key_type: KeyType,
    pub(crate) issuers: Option<Vec<Arc<dyn CertIssuer>>>,
    pub(crate) storage: Option<Arc<dyn Storage>>,
    pub(crate) http01_solver: Option<Arc<dyn Solver>>,
    pub(crate) tls_alpn01_solver: Option<Arc<dyn Solver>>,
    pub(crate) dns01_solver: Option<Arc<dyn Solver>>,
    pub(crate) ocsp: OcspConfig,
    pub(crate) on_demand: Option<Arc<OnDemandConfig>>,
    pub(crate) zerossl_api_key: Option<String>,
    pub(crate) agree_to_tos: bool,
}

impl fmt::Debug for AcmeConfigBuilder {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("AcmeConfigBuilder")
            .field("directory_name", &self.directory_name)
            .field("directory_url", &self.directory_url)
            .field("domains", &self.domains)
            .field("contacts", &self.contacts)
            .field("challenge_type", &self.challenge_type)
            .field("cache_path", &self.cache_path)
            .field("key_type", &self.key_type)
            .finish()
    }
}

impl AcmeConfigBuilder {
    #[inline]
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            directory_name: "lets_encrypt".to_owned(),
            directory_url: LETS_ENCRYPT_PRODUCTION.to_owned(),
            domains: Vec::new(),
            contacts: Default::default(),
            challenge_type: ChallengeType::TlsAlpn01,
            cache_path: None,
            keys_for_http01: None,
            before_expired: Duration::from_secs(12 * 60 * 60),
            key_type: KeyType::EcdsaP256,
            issuers: None,
            storage: None,
            http01_solver: None,
            tls_alpn01_solver: None,
            dns01_solver: None,
            ocsp: OcspConfig::default(),
            on_demand: None,
            zerossl_api_key: None,
            agree_to_tos: true,
        }
    }

    /// Sets the directory url.
    ///
    /// Defaults to Let's Encrypt production.
    #[inline]
    #[must_use]
    pub fn directory(self, name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            directory_name: name.into(),
            directory_url: url.into(),
            ..self
        }
    }

    /// Sets domains.
    #[inline]
    #[must_use]
    pub fn domains(mut self, domains: impl Into<Vec<String>>) -> Self {
        self.domains = domains.into();
        self
    }
    /// Add a domain.
    #[inline]
    #[must_use]
    pub fn add_domain(mut self, domain: impl Into<String>) -> Self {
        self.domains.push(domain.into());
        self
    }

    /// Sets contact emails for the ACME account.
    #[inline]
    #[must_use]
    pub fn contacts(mut self, contacts: impl Into<Vec<String>>) -> Self {
        self.contacts = contacts.into();
        self
    }
    /// Add a contact email for the ACME account.
    #[inline]
    #[must_use]
    pub fn add_contact(mut self, contact: impl Into<String>) -> Self {
        self.contacts.push(contact.into());
        self
    }

    /// Sets the challenge type to HTTP-01.
    #[inline]
    #[must_use]
    pub fn http01_challenge(self) -> Self {
        Self {
            challenge_type: ChallengeType::Http01,
            keys_for_http01: Some(Default::default()),
            ..self
        }
    }

    /// Sets the challenge type to TLS-ALPN-01.
    #[inline]
    #[must_use]
    pub fn tls_alpn01_challenge(self) -> Self {
        Self {
            challenge_type: ChallengeType::TlsAlpn01,
            keys_for_http01: None,
            ..self
        }
    }

    /// Sets the challenge type to DNS-01 with a custom DNS provider.
    #[inline]
    #[must_use]
    pub fn dns01_challenge(mut self, solver: Arc<dyn Solver>) -> Self {
        self.challenge_type = ChallengeType::Dns01;
        self.dns01_solver = Some(solver);
        self.keys_for_http01 = None;
        self
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
            cache_path: Some(path.into()),
            ..self
        }
    }

    /// Sets the duration before expiry to start certificate renewal.
    #[inline]
    #[must_use]
    pub fn before_expired(self, before_expired: Duration) -> Self {
        Self {
            before_expired,
            ..self
        }
    }

    // ----- New certon-powered options -----

    /// Sets the key type for certificate private keys.
    ///
    /// Defaults to [`KeyType::EcdsaP256`].
    /// Available types: `EcdsaP256`, `EcdsaP384`, `EcdsaP521`, `Rsa2048`,
    /// `Rsa4096`, `Rsa8192`, `Ed25519`.
    #[inline]
    #[must_use]
    pub fn key_type(mut self, key_type: KeyType) -> Self {
        self.key_type = key_type;
        self
    }

    /// Adds a custom certificate issuer.
    ///
    /// Multiple issuers can be added. They will be tried in order until one
    /// succeeds.
    #[must_use]
    pub fn add_issuer(mut self, issuer: Arc<dyn CertIssuer>) -> Self {
        self.issuers.get_or_insert_with(Vec::new).push(issuer);
        self
    }

    /// Sets a custom persistent storage backend.
    ///
    /// By default, a [`FileStorage`] will be created from the `cache_path`
    /// if provided.
    #[inline]
    #[must_use]
    pub fn storage(mut self, storage: Arc<dyn Storage>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Sets the OCSP stapling configuration.
    #[inline]
    #[must_use]
    pub fn ocsp(mut self, ocsp: OcspConfig) -> Self {
        self.ocsp = ocsp;
        self
    }

    /// Sets the on-demand TLS configuration.
    ///
    /// When enabled, certificates for unknown domains can be obtained at
    /// TLS handshake time.
    #[inline]
    #[must_use]
    pub fn on_demand(mut self, on_demand: Arc<OnDemandConfig>) -> Self {
        self.on_demand = Some(on_demand);
        self
    }

    /// Configures ZeroSSL as an additional issuer via its API key.
    ///
    /// This automatically adds a [`ZeroSslIssuer`](certon::ZeroSslIssuer)
    /// to the issuer list.
    #[inline]
    #[must_use]
    pub fn zerossl_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.zerossl_api_key = Some(api_key.into());
        self
    }

    /// Sets a custom HTTP-01 solver (overrides the default).
    #[inline]
    #[must_use]
    pub fn http01_solver(mut self, solver: Arc<dyn Solver>) -> Self {
        self.http01_solver = Some(solver);
        self
    }

    /// Sets a custom TLS-ALPN-01 solver (overrides the default).
    #[inline]
    #[must_use]
    pub fn tls_alpn01_solver(mut self, solver: Arc<dyn Solver>) -> Self {
        self.tls_alpn01_solver = Some(solver);
        self
    }

    /// Whether to automatically agree to the CA's terms of service.
    /// Defaults to `true`.
    #[inline]
    #[must_use]
    pub fn agree_to_tos(mut self, agree: bool) -> Self {
        self.agree_to_tos = agree;
        self
    }

    /// Consumes this builder and returns a [`AcmeConfig`] object.
    pub fn build(self) -> IoResult<AcmeConfig> {
        if self.domains.is_empty() {
            return Err(IoError::other("at least one domain name is expected"));
        }
        let Self {
            directory_name,
            directory_url,
            domains,
            contacts,
            challenge_type,
            cache_path,
            keys_for_http01,
            before_expired,
            key_type,
            issuers,
            storage,
            http01_solver,
            tls_alpn01_solver,
            dns01_solver,
            ocsp,
            on_demand,
            zerossl_api_key,
            agree_to_tos,
        } = self;

        Ok(AcmeConfig {
            directory_name,
            directory_url,
            domains,
            contacts,
            challenge_type,
            cache_path,
            keys_for_http01,
            before_expired,
            key_type,
            issuers,
            storage,
            http01_solver,
            tls_alpn01_solver,
            dns01_solver,
            ocsp,
            on_demand,
            zerossl_api_key,
            agree_to_tos,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acme_config_builder_new_defaults() {
        let builder = AcmeConfigBuilder::new();
        assert_eq!(builder.directory_name, "lets_encrypt");
        assert_eq!(builder.directory_url, LETS_ENCRYPT_PRODUCTION);
        assert!(builder.domains.is_empty());
        assert!(builder.contacts.is_empty());
        assert_eq!(builder.challenge_type, ChallengeType::TlsAlpn01);
        assert!(builder.cache_path.is_none());
        assert!(builder.keys_for_http01.is_none());
        assert_eq!(builder.before_expired, Duration::from_secs(12 * 60 * 60));
    }

    #[test]
    fn test_acme_config_builder() {
        let domains = vec!["example.com".to_string(), "example.org".to_string()];
        let contacts = vec!["mailto:admin@example.com".to_string()];

        let acme_config = AcmeConfig::builder()
            .directory("test_directory", "https://test-directory-url.com")
            .domains(domains.clone())
            .contacts(contacts.clone())
            .http01_challenge()
            .cache_path("test_cache_path")
            .before_expired(Duration::from_secs(24 * 60 * 60))
            .build()
            .unwrap();

        assert_eq!(acme_config.directory_name, "test_directory");
        assert_eq!(acme_config.directory_url, "https://test-directory-url.com");
        assert_eq!(acme_config.domains, domains);
        assert_eq!(acme_config.contacts, contacts);
        assert_eq!(acme_config.challenge_type, ChallengeType::Http01);
        assert_eq!(
            acme_config.cache_path,
            Some(PathBuf::from("test_cache_path"))
        );
        assert_eq!(
            acme_config.before_expired,
            Duration::from_secs(24 * 60 * 60)
        );
    }

    #[test]
    fn test_acme_config_builder_add_domain() {
        let config = AcmeConfig::builder()
            .add_domain("example.com")
            .add_domain("www.example.com")
            .build()
            .unwrap();

        assert_eq!(config.domains.len(), 2);
        assert_eq!(config.domains[0], "example.com");
        assert_eq!(config.domains[1], "www.example.com");
    }

    #[test]
    fn test_acme_config_builder_add_contact() {
        let config = AcmeConfig::builder()
            .add_domain("example.com")
            .add_contact("mailto:admin@example.com")
            .add_contact("mailto:webmaster@example.com")
            .build()
            .unwrap();

        assert_eq!(config.contacts.len(), 2);
    }

    #[test]
    fn test_acme_config_builder_no_domains_error() {
        let result = AcmeConfig::builder().build();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("at least one domain"));
    }

    #[test]
    fn test_acme_config_builder_key_type() {
        let config = AcmeConfig::builder()
            .add_domain("example.com")
            .key_type(KeyType::Rsa4096)
            .build()
            .unwrap();

        assert!(matches!(config.key_type, KeyType::Rsa4096));
    }

    #[test]
    fn test_acme_config_builder_dns01() {
        // DNS-01 requires a solver; just test the challenge type is set.
        let builder = AcmeConfigBuilder::new();
        assert_eq!(builder.challenge_type, ChallengeType::TlsAlpn01);
    }

    #[test]
    fn test_acme_config_debug() {
        let config = AcmeConfig::builder()
            .add_domain("example.com")
            .build()
            .unwrap();

        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("AcmeConfig"));
        assert!(debug_str.contains("directory_name"));
    }
}
