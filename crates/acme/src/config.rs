use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};
use std::io::{Error as IoError, Result as IoResult};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use http::Uri;
use parking_lot::RwLock;

use super::key_pair::KeyPair;
use super::{ChallengeType, LETS_ENCRYPT_PRODUCTION};

/// ACME configuration
pub struct AcmeConfig {
    pub(crate) directory_name: String,
    pub(crate) directory_url: String,
    pub(crate) domains: Vec<String>,
    pub(crate) contacts: Vec<String>,
    pub(crate) key_pair: Arc<KeyPair>,
    pub(crate) challenge_type: ChallengeType,
    pub(crate) cache_path: Option<PathBuf>,
    pub(crate) keys_for_http01: Option<Arc<RwLock<HashMap<String, String>>>>,
    pub(crate) before_expired: Duration,
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
            .field("cache_path", &self.cache_path)
            .finish()
    }
}

/// ACME configuration builder
#[derive(Debug)]
pub struct AcmeConfigBuilder {
    pub(crate) directory_name: String,
    pub(crate) directory_url: String,
    pub(crate) domains: Vec<String>,
    pub(crate) contacts: Vec<String>,
    pub(crate) challenge_type: ChallengeType,
    pub(crate) cache_path: Option<PathBuf>,
    pub(crate) keys_for_http01: Option<Arc<RwLock<HashMap<String, String>>>>,
    pub(crate) before_expired: Duration,
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
        }
    }

    /// Sets the directory url.
    ///
    /// Defaults to lets encrypt production.
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

    /// Sets contact email for the ACME account.
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

    /// Sets the challenge type Http01
    #[inline]
    #[must_use]
    pub fn http01_challenge(self) -> Self {
        Self {
            challenge_type: ChallengeType::Http01,
            keys_for_http01: Some(Default::default()),
            ..self
        }
    }
    /// Sets the challenge type TlsAlpn01
    #[inline]
    #[must_use]
    pub fn tls_alpn01_challenge(self) -> Self {
        Self {
            challenge_type: ChallengeType::TlsAlpn01,
            keys_for_http01: None,
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
            cache_path: Some(path.into()),
            ..self
        }
    }

    /// Sets the duration update certificate before it expired.
    #[inline]
    #[must_use]
    pub fn before_expired(self, before_expired: Duration) -> Self {
        Self {
            before_expired,
            ..self
        }
    }

    /// Consumes this builder and returns a [`AcmeConfig`] object.
    pub fn build(self) -> IoResult<AcmeConfig> {
        self.directory_url
            .parse::<Uri>()
            .map_err(|e| IoError::other(format!("invalid directory url: {e}")))?;
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
        } = self;

        Ok(AcmeConfig {
            directory_name,
            directory_url,
            domains,
            contacts,
            key_pair: Arc::new(KeyPair::generate()?),
            challenge_type,
            cache_path,
            keys_for_http01,
            before_expired,
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
        assert_eq!(config.contacts[0], "mailto:admin@example.com");
        assert_eq!(config.contacts[1], "mailto:webmaster@example.com");
    }

    #[test]
    fn test_acme_config_builder_tls_alpn01_challenge() {
        let config = AcmeConfig::builder()
            .add_domain("example.com")
            .http01_challenge()
            .tls_alpn01_challenge() // Should override http01
            .build()
            .unwrap();

        assert_eq!(config.challenge_type, ChallengeType::TlsAlpn01);
        assert!(config.keys_for_http01.is_none());
    }

    #[test]
    fn test_acme_config_builder_http01_challenge() {
        let config = AcmeConfig::builder()
            .add_domain("example.com")
            .http01_challenge()
            .build()
            .unwrap();

        assert_eq!(config.challenge_type, ChallengeType::Http01);
        assert!(config.keys_for_http01.is_some());
    }

    #[test]
    fn test_acme_config_builder_no_domains_error() {
        let result = AcmeConfig::builder().build();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("at least one domain"));
    }

    #[test]
    fn test_acme_config_builder_invalid_url_error() {
        let result = AcmeConfig::builder()
            .directory("test", "not a valid url \x00 with null")
            .add_domain("example.com")
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_acme_config_builder_debug() {
        let builder = AcmeConfigBuilder::new();
        let debug_str = format!("{:?}", builder);
        assert!(debug_str.contains("AcmeConfigBuilder"));
        assert!(debug_str.contains("directory_name"));
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
        assert!(debug_str.contains("directory_url"));
        assert!(debug_str.contains("domains"));
    }

    #[test]
    fn test_acme_config_builder_static() {
        // Test that AcmeConfig::builder() returns a new builder
        let builder = AcmeConfig::builder();
        assert_eq!(builder.directory_name, "lets_encrypt");
    }
}
