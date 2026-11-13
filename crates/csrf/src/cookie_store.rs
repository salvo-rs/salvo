use cookie::time::Duration;
use cookie::{Cookie, Expiration, SameSite};
use salvo_core::http::SecureCookiePolicy;
use salvo_core::{Depot, Error, Request, Response};

use super::CsrfStore;
use crate::CsrfCipher;

/// A `CsrfStore` implementation that stores the CSRF proof in a cookie.
///
/// This is a double-submit cookie, not a token bound to an authentication session.
/// Use [`Self::host_prefixed`] on HTTPS deployments to prevent sibling subdomains
/// from injecting a parent-domain CSRF cookie, or use `SessionStore`
/// when the token must be bound to a server-side session.
#[derive(Debug)]
#[non_exhaustive]
pub struct CookieStore {
    /// CSRF cookie ttl.
    pub ttl: Duration,
    /// CSRF cookie name.
    pub name: String,
    /// CSRF cookie path.
    pub path: String,
    /// CSRF cookie domain.
    pub domain: Option<String>,
    /// Forced secure cookie attribute. If None, infer from request URI scheme.
    pub secure: Option<bool>,
    /// Policy used when `secure` is not explicitly set.
    pub secure_cookie_policy: SecureCookiePolicy,
}
impl Default for CookieStore {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl CookieStore {
    /// Creates a host-only CSRF cookie for HTTPS deployments.
    ///
    /// The browser enforces the `__Host-` prefix: `Secure`, `Path=/`, and no
    /// `Domain`. Invalid combinations (including public-field changes) are
    /// rejected on save and ignored on load. TLS-terminating proxies may use
    /// this constructor because it always emits `Secure` independently of the
    /// internal connection scheme. The browser-facing origin must use HTTPS.
    ///
    /// This prevents parent-domain cookie injection; it does not bind tokens to
    /// a login session. Renaming the cookie without the prefix opts out of the
    /// browser's host-prefix protection. Loads match the original request Cookie
    /// name exactly, ignoring percent-encoded aliases and cookie-jar overrides.
    #[must_use]
    pub fn host_prefixed() -> Self {
        Self::new().name("__Host-salvo.csrf").secure(true)
    }

    /// Create a new `CookieStore`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ttl: Duration::days(1),
            name: "salvo.csrf".into(),
            path: "/".into(),
            domain: None,
            secure: None,
            secure_cookie_policy: SecureCookiePolicy::AutoFromScheme,
        }
    }
    /// Sets cookie name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Sets cookie ttl.
    #[must_use]
    pub fn ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Sets cookie path.
    #[must_use]
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Sets cookie domain.
    #[must_use]
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// Forces the `Secure` attribute for CSRF cookies.
    ///
    /// By default this is detected from the request URI scheme. Use this option in
    /// production when TLS is terminated before the Salvo application.
    #[must_use]
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = Some(secure);
        self.secure_cookie_policy = SecureCookiePolicy::from_bool(secure);
        self
    }

    /// Sets the policy used to decide whether CSRF cookies include `Secure`.
    ///
    /// This clears the legacy fixed `secure` override so the policy controls future saves.
    #[must_use]
    pub fn secure_cookie_policy(mut self, policy: SecureCookiePolicy) -> Self {
        self.secure = None;
        self.secure_cookie_policy = policy;
        self
    }

    fn cookie_is_secure(&self, req: &Request) -> bool {
        self.secure
            .map(SecureCookiePolicy::from_bool)
            .unwrap_or(self.secure_cookie_policy)
            .is_secure(req)
    }

    fn validate_prefix(&self, secure: bool) -> Result<(), Error> {
        if self.name.starts_with("__Host-")
            && (!secure || self.path != "/" || self.domain.is_some())
        {
            return Err(Error::other(
                "__Host- CSRF cookies require Secure, Path=/, and no Domain",
            ));
        }
        Ok(())
    }
}
impl CsrfStore for CookieStore {
    type Error = Error;
    async fn load<C: CsrfCipher>(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        cipher: &C,
    ) -> Option<(String, String)> {
        self.validate_prefix(self.cookie_is_secure(req)).ok()?;
        let parsed;
        let cookie = if self.name.starts_with("__Host-") {
            // The general cookie jar decodes percent escapes in names. Browsers
            // enforce prefixes on the original name, so an encoded alias must
            // never replace a genuinely prefixed cookie in this store.
            let mut exact = req
                .headers()
                .get_all(salvo_core::http::header::COOKIE)
                .iter()
                .filter_map(|header| header.to_str().ok())
                .flat_map(|header| header.split(';'))
                .map(str::trim)
                .filter(|raw| {
                    raw.split_once('=')
                        .is_some_and(|(name, _)| name.trim() == self.name)
                });
            let raw = exact.next()?;
            if exact.next().is_some() {
                return None;
            }
            parsed = Cookie::parse_encoded(raw).ok()?;
            &parsed
        } else {
            req.cookie(&self.name)?
        };
        let (token, proof) = cookie.value().split_once('.')?;
        cipher
            .verify_async(token, proof)
            .await
            .then(|| (token.into(), proof.into()))
    }
    async fn save(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        token: &str,
        proof: &str,
    ) -> Result<(), Self::Error> {
        let secure = self.cookie_is_secure(req);
        self.validate_prefix(secure)?;
        let expires = cookie::time::OffsetDateTime::now_utc() + self.ttl;
        let cookie_builder = Cookie::build((self.name.clone(), format!("{token}.{proof}")))
            .http_only(true)
            .same_site(SameSite::Strict)
            .path(self.path.clone())
            .secure(secure)
            .expires(Expiration::DateTime(expires));
        let cookie = if let Some(domain) = &self.domain {
            cookie_builder.domain(domain.clone()).build()
        } else {
            cookie_builder.build()
        };
        res.add_cookie(cookie);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::test::TestClient;

    use super::*;
    use crate::bcrypt_cipher::BcryptCipher;

    #[tokio::test]
    async fn host_prefixed_cookie_has_browser_enforced_scope() {
        let store = CookieStore::host_prefixed();
        let mut req = TestClient::get("http://internal-proxy/").build();
        let mut res = Response::new();
        store
            .save(&mut req, &mut Depot::new(), &mut res, "token", "proof")
            .await
            .unwrap();
        let cookie = res.cookies().get("__Host-salvo.csrf").unwrap();
        assert_eq!(cookie.secure(), Some(true));
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.domain(), None);
        assert_eq!(cookie.http_only(), Some(true));
    }

    #[tokio::test]
    async fn host_prefix_uses_original_cookie_names_not_decoded_jar_aliases() {
        let store = CookieStore::host_prefixed();
        let cipher = BcryptCipher::new().cost(4);
        let (victim, victim_proof) = cipher.generate();
        let (attacker, attacker_proof) = cipher.generate();
        let genuine = format!("__Host-salvo.csrf={victim}.{victim_proof}");
        for name in [
            "%5F_Host-salvo.csrf",
            "__Host-salvo%2Ecsrf",
            "__%48ost-salvo.csrf",
        ] {
            let alias = format!("{name}={attacker}.{attacker_proof}");
            let mut req = TestClient::get("https://app.example.com/")
                .add_header("cookie", &genuine, true)
                .add_header("cookie", &alias, false)
                .build();
            // Exercise the actual Request parser: the alias shadows the genuine
            // cookie in its decoded jar, but must not affect prefixed-store load.
            assert_eq!(
                req.cookie(&store.name).unwrap().value(),
                format!("{attacker}.{attacker_proof}")
            );
            assert_eq!(
                store.load(&mut req, &mut Depot::new(), &cipher).await,
                Some((victim.clone(), victim_proof.clone()))
            );
            let mut req = TestClient::get("https://app.example.com/")
                .add_header("cookie", &alias, true)
                .build();
            assert!(
                store
                    .load(&mut req, &mut Depot::new(), &cipher)
                    .await
                    .is_none()
            );
            let mut req = TestClient::get("https://app.example.com/")
                .add_header("cookie", format!("{alias}; {genuine}"), true)
                .build();
            assert_eq!(
                store.load(&mut req, &mut Depot::new(), &cipher).await,
                Some((victim.clone(), victim_proof.clone()))
            );
        }
        let mut req = TestClient::get("https://app.example.com/")
            .add_header("cookie", format!("{genuine}; {genuine}"), true)
            .build();
        assert!(
            store
                .load(&mut req, &mut Depot::new(), &cipher)
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn invalid_host_prefix_configuration_fails_closed() {
        let cipher = BcryptCipher::new().cost(4);
        let (token, proof) = cipher.generate();
        for store in [
            CookieStore::host_prefixed().domain("example.com"),
            CookieStore::host_prefixed().path("/nested"),
            CookieStore::host_prefixed().secure(false),
            CookieStore::host_prefixed().secure_cookie_policy(SecureCookiePolicy::Never),
        ] {
            let mut req = TestClient::get("https://example.com/").build();
            req.cookies_mut()
                .add(Cookie::new("__Host-salvo.csrf", format!("{token}.{proof}")));
            let mut depot = Depot::new();
            let mut res = Response::new();
            assert!(store.load(&mut req, &mut depot, &cipher).await.is_none());
            assert!(
                store
                    .save(&mut req, &mut depot, &mut res, &token, &proof)
                    .await
                    .is_err()
            );
            assert!(res.cookies().get("__Host-salvo.csrf").is_none());
        }
    }

    #[tokio::test]
    async fn test_cookie_store() {
        let cipher = BcryptCipher::new();
        let cookie_store = CookieStore::new()
            .name("test_cookie")
            .ttl(Duration::days(1))
            .path("/test")
            .domain("example.com")
            .secure(true);

        assert_eq!(cookie_store.name, "test_cookie");
        assert_eq!(cookie_store.ttl, Duration::days(1));
        assert_eq!(cookie_store.path, "/test");
        assert_eq!(cookie_store.domain.as_deref(), Some("example.com"));
        assert_eq!(cookie_store.secure, Some(true));
        assert_eq!(
            cookie_store.secure_cookie_policy,
            SecureCookiePolicy::Always
        );

        let mut req = TestClient::get("http://example.com/test").build();
        let mut depot = Depot::new();
        let mut res = Response::new();

        let (token, proof) = cipher.generate();
        cookie_store
            .save(&mut req, &mut depot, &mut res, &token, &proof)
            .await
            .unwrap();

        let cookie = res.cookies().get("test_cookie").unwrap();
        assert_eq!(cookie.name(), "test_cookie");
        assert_eq!(cookie.path(), Some("/test"));
        assert_eq!(cookie.domain(), Some("example.com"));
        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(cookie.same_site(), Some(SameSite::Strict));
        assert_eq!(cookie.secure(), Some(true));

        req.cookies_mut().add(cookie.clone());

        let loaded = cookie_store.load(&mut req, &mut depot, &cipher).await;
        assert_eq!(loaded, Some((token, proof)));
    }

    #[tokio::test]
    async fn test_cookie_store_infers_secure_from_https_scheme() {
        let cipher = BcryptCipher::new();
        let cookie_store = CookieStore::new().name("test_cookie");
        let mut req = TestClient::get("https://example.com/test").build();
        let mut depot = Depot::new();
        let mut res = Response::new();

        let (token, proof) = cipher.generate();
        cookie_store
            .save(&mut req, &mut depot, &mut res, &token, &proof)
            .await
            .unwrap();

        let cookie = res.cookies().get("test_cookie").unwrap();
        assert_eq!(cookie.secure(), Some(true));
    }

    #[tokio::test]
    async fn test_cookie_store_secure_policy_can_force_secure() {
        let cipher = BcryptCipher::new();
        let cookie_store = CookieStore::new()
            .name("test_cookie")
            .secure_cookie_policy(SecureCookiePolicy::Always);
        let mut req = TestClient::get("http://example.com/test").build();
        let mut depot = Depot::new();
        let mut res = Response::new();

        let (token, proof) = cipher.generate();
        cookie_store
            .save(&mut req, &mut depot, &mut res, &token, &proof)
            .await
            .unwrap();

        let cookie = res.cookies().get("test_cookie").unwrap();
        assert_eq!(cookie.secure(), Some(true));
    }

    #[tokio::test]
    async fn test_cookie_store_secure_policy_can_disable_https_secure() {
        let cipher = BcryptCipher::new();
        let cookie_store = CookieStore::new()
            .name("test_cookie")
            .secure_cookie_policy(SecureCookiePolicy::Never);
        let mut req = TestClient::get("https://example.com/test").build();
        let mut depot = Depot::new();
        let mut res = Response::new();

        let (token, proof) = cipher.generate();
        cookie_store
            .save(&mut req, &mut depot, &mut res, &token, &proof)
            .await
            .unwrap();

        let cookie = res.cookies().get("test_cookie").unwrap();
        assert_eq!(cookie.secure(), Some(false));
    }
}
