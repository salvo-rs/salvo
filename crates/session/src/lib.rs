//! # Salvo Session Support
//!
//! Salvo's session middleware is built on top of
//! [`saysion`](https://github.com/salvo-rs/saysion).
//!
//! See a complete example: [`session-login`](https://github.com/salvo-rs/salvo/tree/main/examples/session-login)
//!
//! Sessions allow Salvo applications to securely attach data to browser sessions,
//! enabling retrieval and modification of this data on subsequent visits.
//! Session data is typically retained only for the duration of a browser session.
//!
//! ## Stores
//!
//! It is highly recommended to use an external-datastore-backed session storage
//! for production Salvo applications. For a list of currently available session
//! stores, see [the documentation for saysion](https://github.com/salvo-rs/saysion).
//!
//! ## Security
//!
//! While each session store may have different security implications,
//! Salvo's session system works as follows:
//!
//! On each request, Salvo checks for the cookie specified by `cookie_name`
//! in the handler configuration.
//!
//! ### When no cookie is found:
//!
//! 1. A cryptographically random cookie value is generated
//! 2. A cookie is set on the outbound response and signed with an HKDF key derived from the
//!    `secret` provided when creating the SessionHandler
//! 3. The session store uses a SHA256 digest of the cookie value to store the session along with an
//!    optional expiry time
//!
//! ### When a cookie is found:
//!
//! 1. The HKDF-derived signing key verifies the cookie value's signature
//! 2. If verification succeeds, the value is passed to the session store to retrieve the associated
//!    Session
//! 3. For most session stores, this involves taking a SHA256 digest of the cookie value and
//!    retrieving a serialized Session from an external datastore
//!
//! ### Expiry Handling
//!
//! Sessions include expiry information in both the cookie and the serialization format.
//! Even if an adversary tampers with a cookie's expiry, Salvo validates
//! the expiry on the contained session before using it.
//!
//! ### Error Handling
//!
//! If any failures occur during session retrieval, a new empty session
//! is generated for the request, which proceeds through the application normally.
//!
//! ## Stale/Expired Session Cleanup
//!
//! Any session store (except the cookie store) will accumulate stale sessions over time.
//! Although Salvo ensures expired sessions won't be used, it remains the
//! application's responsibility to periodically call cleanup on the session
//! store if required.
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::fmt::{self, Formatter};
use std::time::Duration;

use cookie::{Cookie, Key, SameSite};
use salvo_core::http::uri::Scheme;
use salvo_core::{Depot, Error, FlowCtrl, Handler, Request, Response, async_trait};
use saysion::base64::Engine as _;
use saysion::base64::engine::general_purpose;
use saysion::hmac::{Hmac, Mac};
use saysion::sha2::Sha256;
pub use saysion::{CookieStore, MemoryStore, Session, SessionStore};

/// Key for store data in depot.
pub const SESSION_KEY: &str = "::salvo::session";
const BASE64_DIGEST_LEN: usize = 44;

/// Trait for `Depot` to get and set session.
pub trait SessionDepotExt {
    /// Sets session
    fn set_session(&mut self, session: Session) -> &mut Self;
    /// Take session
    fn take_session(&mut self) -> Option<Session>;
    /// Get session reference
    fn session(&self) -> Option<&Session>;
    /// Get session mutable reference
    fn session_mut(&mut self) -> Option<&mut Session>;
}

impl SessionDepotExt for Depot {
    #[inline]
    fn set_session(&mut self, session: Session) -> &mut Self {
        self.insert(SESSION_KEY, session);
        self
    }
    #[inline]
    fn take_session(&mut self) -> Option<Session> {
        self.remove(SESSION_KEY).ok()
    }
    #[inline]
    fn session(&self) -> Option<&Session> {
        self.get(SESSION_KEY).ok()
    }
    #[inline]
    fn session_mut(&mut self) -> Option<&mut Session> {
        self.get_mut(SESSION_KEY).ok()
    }
}

/// `HandlerBuilder` is a builder for [`SessionHandler`].
pub struct HandlerBuilder<S> {
    store: S,
    cookie_path: String,
    cookie_name: String,
    cookie_domain: Option<String>,
    session_ttl: Option<Duration>,
    save_unchanged: bool,
    same_site_policy: SameSite,
    key: Key,
    fallback_keys: Vec<Key>,
}
impl<S> fmt::Debug for HandlerBuilder<S>
where
    S: SessionStore + fmt::Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandlerBuilder")
            .field("store", &self.store)
            .field("cookie_path", &self.cookie_path)
            .field("cookie_name", &self.cookie_name)
            .field("cookie_domain", &self.cookie_domain)
            .field("session_ttl", &self.session_ttl)
            .field("same_site_policy", &self.same_site_policy)
            .field("key", &"..")
            .field("fallback_keys", &"..")
            .field("save_unchanged", &self.save_unchanged)
            .finish()
    }
}

/// Minimum recommended secret key length in bytes (256 bits).
pub const RECOMMENDED_KEY_LEN: usize = 32;

impl<S> HandlerBuilder<S>
where
    S: SessionStore,
{
    /// Create new `HandlerBuilder`
    ///
    /// # Security Note
    ///
    /// The `secret` should be at least 32 bytes (256 bits) for adequate security.
    /// A warning will be logged if a shorter key is provided.
    ///
    /// **Example of generating a secure key:**
    /// ```ignore
    /// use rand::Rng;
    /// let mut key = [0u8; 64];
    /// rand::rngs::SysRng.fill_bytes(&mut key);
    /// ```
    #[inline]
    #[must_use]
    pub fn new(store: S, secret: &[u8]) -> Self {
        if secret.len() < RECOMMENDED_KEY_LEN {
            tracing::warn!(
                "Session secret key is {} bytes, but at least {} bytes is recommended for security",
                secret.len(),
                RECOMMENDED_KEY_LEN
            );
        }
        Self {
            store,
            save_unchanged: true,
            cookie_path: "/".into(),
            cookie_name: "salvo.session.id".into(),
            cookie_domain: None,
            same_site_policy: SameSite::Lax,
            session_ttl: Some(Duration::from_secs(24 * 60 * 60)),
            key: Key::from(secret),
            fallback_keys: vec![],
        }
    }

    /// Sets a cookie path for this session middleware.
    ///
    /// The default for this value is "/".
    #[inline]
    #[must_use]
    pub fn cookie_path(mut self, cookie_path: impl Into<String>) -> Self {
        self.cookie_path = cookie_path.into();
        self
    }

    /// Sets a session ttl. This will be used both for the cookie
    /// expiry and also for the session-internal expiry.
    ///
    /// The default for this value is one day. Set this to None to not
    /// set a cookie or session expiry. This is not recommended.
    #[inline]
    #[must_use]
    pub fn session_ttl(mut self, session_ttl: Option<Duration>) -> Self {
        self.session_ttl = session_ttl;
        self
    }

    /// Sets the name of the cookie that the session is stored with or in.
    ///
    /// If you are running multiple tide applications on the same
    /// domain, you will need different values for each
    /// application. The default value is "salvo.session_id".
    #[inline]
    #[must_use]
    pub fn cookie_name(mut self, cookie_name: impl Into<String>) -> Self {
        self.cookie_name = cookie_name.into();
        self
    }

    /// Sets the `save_unchanged` value.
    ///
    /// When `save_unchanged` is enabled, a session will cookie will always be set.
    ///
    /// With `save_unchanged` disabled, the session data must be modified
    /// from the `Default` value in order for it to save. If a session
    /// already exists and its data unmodified in the course of a
    /// request, the session will only be persisted if
    /// `save_unchanged` is enabled.
    #[inline]
    #[must_use]
    pub fn save_unchanged(mut self, value: bool) -> Self {
        self.save_unchanged = value;
        self
    }

    /// Sets the same site policy for the session cookie. Defaults to
    /// SameSite::Lax. See [incrementally better
    /// cookies](https://tools.ietf.org/html/draft-west-cookie-incrementalism-01)
    /// for more information about this setting.
    #[inline]
    #[must_use]
    pub fn same_site_policy(mut self, policy: SameSite) -> Self {
        self.same_site_policy = policy;
        self
    }

    /// Sets the domain of the cookie.
    #[inline]
    #[must_use]
    pub fn cookie_domain(mut self, cookie_domain: impl AsRef<str>) -> Self {
        self.cookie_domain = Some(cookie_domain.as_ref().to_owned());
        self
    }
    /// Sets fallbacks.
    #[inline]
    #[must_use]
    pub fn fallback_keys(mut self, keys: Vec<impl Into<Key>>) -> Self {
        self.fallback_keys = keys.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Add fallback secret.
    #[inline]
    #[must_use]
    pub fn add_fallback_key(mut self, key: impl Into<Key>) -> Self {
        self.fallback_keys.push(key.into());
        self
    }

    /// Build `SessionHandler`
    pub fn build(self) -> Result<SessionHandler<S>, Error> {
        let Self {
            store,
            save_unchanged,
            cookie_path,
            cookie_name,
            cookie_domain,
            session_ttl,
            same_site_policy,
            key,
            fallback_keys,
        } = self;
        let hmac = Hmac::<Sha256>::new_from_slice(key.signing())
            .map_err(|_| Error::Other("invalid key length".into()))?;
        let fallback_hmacs = fallback_keys
            .iter()
            .map(|key| Hmac::<Sha256>::new_from_slice(key.signing()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Error::Other("invalid key length".into()))?;
        Ok(SessionHandler {
            store,
            save_unchanged,
            cookie_path,
            cookie_name,
            cookie_domain,
            session_ttl,
            same_site_policy,
            hmac,
            fallback_hmacs,
        })
    }
}

/// `SessionHandler` is a middleware for session.
pub struct SessionHandler<S> {
    store: S,
    cookie_path: String,
    cookie_name: String,
    cookie_domain: Option<String>,
    session_ttl: Option<Duration>,
    save_unchanged: bool,
    same_site_policy: SameSite,
    hmac: Hmac<Sha256>,
    fallback_hmacs: Vec<Hmac<Sha256>>,
}
impl<S> fmt::Debug for SessionHandler<S>
where
    S: SessionStore + fmt::Debug,
{
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionHandler")
            .field("store", &self.store)
            .field("cookie_path", &self.cookie_path)
            .field("cookie_name", &self.cookie_name)
            .field("cookie_domain", &self.cookie_domain)
            .field("session_ttl", &self.session_ttl)
            .field("same_site_policy", &self.same_site_policy)
            .field("key", &"..")
            .field("fallback_keys", &"..")
            .field("save_unchanged", &self.save_unchanged)
            .finish()
    }
}
#[async_trait]
impl<S> Handler for SessionHandler<S>
where
    S: SessionStore + Send + Sync + 'static,
{
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let cookie = req.cookies().get(&self.cookie_name);
        let cookie_value = cookie.and_then(|cookie| self.verify_signature(cookie.value()).ok());

        let mut session = self.load_or_create(cookie_value).await;

        if let Some(ttl) = self.session_ttl {
            session.expire_in(ttl);
        }

        depot.set_session(session);

        ctrl.call_next(req, depot, res).await;
        if ctrl.is_ceased() {
            return;
        }

        let session = depot.take_session().expect("session should exist in depot");
        if session.is_destroyed() {
            if let Err(e) = self.store.destroy_session(session).await {
                tracing::error!(error = ?e, "unable to destroy session");
            }
            res.remove_cookie(&self.cookie_name);
        } else if self.save_unchanged || session.data_changed() {
            match self.store.store_session(session).await {
                Ok(cookie_value) => {
                    if let Some(cookie_value) = cookie_value {
                        let secure_cookie = req.uri().scheme() == Some(&Scheme::HTTPS);
                        let cookie = self.build_cookie(secure_cookie, cookie_value);
                        res.add_cookie(cookie);
                    }
                }
                Err(e) => {
                    tracing::error!(error = ?e, "store session error");
                }
            }
        }
    }
}

impl<S> SessionHandler<S>
where
    S: SessionStore + Send + Sync + 'static,
{
    /// Create new `HandlerBuilder`
    pub fn builder(store: S, secret: &[u8]) -> HandlerBuilder<S> {
        HandlerBuilder::new(store, secret)
    }
    #[inline]
    async fn load_or_create(&self, cookie_value: Option<String>) -> Session {
        let session = match cookie_value {
            Some(cookie_value) => self.store.load_session(cookie_value).await.ok().flatten(),
            None => None,
        };

        session
            .and_then(|session| session.validate())
            .unwrap_or_default()
    }
    // the following is reused verbatim from
    // https://github.com/SergioBenitez/cookie-rs/blob/master/src/secure/signed.rs#L51-L66
    /// Given a signed value `str` where the signature is prepended to `value`,
    /// verifies the signed value and returns it. If there's a problem, returns
    /// an `Err` with a string describing the issue.
    fn verify_signature(&self, cookie_value: &str) -> Result<String, Error> {
        if cookie_value.len() < BASE64_DIGEST_LEN {
            return Err(Error::Other(
                "length of value is <= BASE64_DIGEST_LEN".into(),
            ));
        }

        // Split [MAC | original-value] into its two parts.
        let (digest_str, value) = cookie_value.split_at(BASE64_DIGEST_LEN);
        let digest = general_purpose::STANDARD
            .decode(digest_str)
            .map_err(|_| Error::Other("bad base64 digest".into()))?;

        // Perform the verification.
        let mut hmac = self.hmac.clone();
        hmac.update(value.as_bytes());
        if hmac.verify_slice(&digest).is_ok() {
            return Ok(value.to_owned());
        }
        for hmac in &self.fallback_hmacs {
            let mut hmac = hmac.clone();
            hmac.update(value.as_bytes());
            if hmac.verify_slice(&digest).is_ok() {
                return Ok(value.to_owned());
            }
        }
        Err(Error::Other("value did not verify".into()))
    }
    fn build_cookie(&self, secure: bool, cookie_value: String) -> Cookie<'static> {
        let mut cookie = Cookie::build((self.cookie_name.clone(), cookie_value))
            .http_only(true)
            .same_site(self.same_site_policy)
            .secure(secure)
            .path(self.cookie_path.clone())
            .build();

        if let Some(ttl) = self.session_ttl {
            cookie.set_expires(Some((std::time::SystemTime::now() + ttl).into()));
        }

        if let Some(cookie_domain) = self.cookie_domain.clone() {
            cookie.set_domain(cookie_domain)
        }

        self.sign_cookie(&mut cookie);

        cookie
    }
    // The following is reused verbatim from
    // https://github.com/SergioBenitez/cookie-rs/blob/master/src/secure/signed.rs#L37-46
    /// signs the cookie's value providing integrity and authenticity.
    fn sign_cookie(&self, cookie: &mut Cookie<'_>) {
        // Compute HMAC-SHA256 of the cookie's value.
        let mut mac = self.hmac.clone();
        mac.update(cookie.value().as_bytes());

        // Cookie's new value is [MAC | original-value].
        let mut new_value = general_purpose::STANDARD.encode(mac.finalize().into_bytes());
        new_value.push_str(cookie.value());
        cookie.set_value(new_value);
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::http::Method;
    use salvo_core::http::header::*;
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use super::*;

    #[test]
    fn test_session_data() {
        let builder = SessionHandler::builder(
            saysion::CookieStore,
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .cookie_domain("test.domain")
        .cookie_name("test_cookie")
        .cookie_path("/abc")
        .same_site_policy(SameSite::Strict)
        .session_ttl(Some(Duration::from_secs(30)));
        assert!(format!("{builder:?}").contains("test_cookie"));

        let handler = builder.build().unwrap();
        assert!(format!("{handler:?}").contains("test_cookie"));
        assert_eq!(handler.cookie_domain, Some("test.domain".into()));
        assert_eq!(handler.cookie_name, "test_cookie");
        assert_eq!(handler.cookie_path, "/abc");
        assert_eq!(handler.same_site_policy, SameSite::Strict);
        assert_eq!(handler.session_ttl, Some(Duration::from_secs(30)));
    }

    #[tokio::test]
    async fn test_session_login() {
        #[handler]
        pub async fn login(req: &mut Request, depot: &mut Depot, res: &mut Response) {
            if req.method() == Method::POST {
                let mut session = Session::new();
                session
                    .insert("username", req.form::<String>("username").await.unwrap())
                    .unwrap();
                depot.set_session(session);
                res.render(Redirect::other("/"));
            } else {
                res.render(Text::Html("login page"));
            }
        }

        #[handler]
        pub async fn logout(depot: &mut Depot, res: &mut Response) {
            if let Some(session) = depot.session_mut() {
                session.remove("username");
            }
            res.render(Redirect::other("/"));
        }

        #[handler]
        pub async fn home(depot: &mut Depot, res: &mut Response) {
            let mut content = r#"home"#.into();
            if let Some(session) = depot.session_mut() {
                if let Some(username) = session.get::<String>("username") {
                    content = username;
                }
            }
            res.render(Text::Html(content));
        }

        let session_handler = SessionHandler::builder(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .build()
        .unwrap();
        let router = Router::new()
            .hoop(session_handler)
            .get(home)
            .push(Router::with_path("login").get(login).post(login))
            .push(Router::with_path("logout").get(logout));
        let service = Service::new(router);

        let response = TestClient::post("http://127.0.0.1:8698/login")
            .raw_form("username=salvo")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::SEE_OTHER));
        let cookie = response.headers().get(SET_COOKIE).unwrap();

        let mut response = TestClient::get("http://127.0.0.1:8698/")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert_eq!(response.take_string().await.unwrap(), "salvo");

        let response = TestClient::get("http://127.0.0.1:8698/logout")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::SEE_OTHER));

        let mut response = TestClient::get("http://127.0.0.1:8698/")
            .send(&service)
            .await;
        assert_eq!(response.take_string().await.unwrap(), "home");
    }

    // Tests for HandlerBuilder
    #[test]
    fn test_handler_builder_new() {
        let builder = HandlerBuilder::new(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        );
        assert_eq!(builder.cookie_path, "/");
        assert_eq!(builder.cookie_name, "salvo.session.id");
        assert!(builder.cookie_domain.is_none());
        assert!(builder.save_unchanged);
        assert_eq!(builder.same_site_policy, SameSite::Lax);
        assert_eq!(builder.session_ttl, Some(Duration::from_secs(24 * 60 * 60)));
    }

    #[test]
    fn test_handler_builder_cookie_path() {
        let builder = HandlerBuilder::new(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .cookie_path("/custom");
        assert_eq!(builder.cookie_path, "/custom");
    }

    #[test]
    fn test_handler_builder_session_ttl() {
        let builder = HandlerBuilder::new(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .session_ttl(Some(Duration::from_secs(3600)));
        assert_eq!(builder.session_ttl, Some(Duration::from_secs(3600)));
    }

    #[test]
    fn test_handler_builder_session_ttl_none() {
        let builder = HandlerBuilder::new(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .session_ttl(None);
        assert!(builder.session_ttl.is_none());
    }

    #[test]
    fn test_handler_builder_cookie_name() {
        let builder = HandlerBuilder::new(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .cookie_name("my_session");
        assert_eq!(builder.cookie_name, "my_session");
    }

    #[test]
    fn test_handler_builder_save_unchanged() {
        let builder = HandlerBuilder::new(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .save_unchanged(false);
        assert!(!builder.save_unchanged);
    }

    #[test]
    fn test_handler_builder_same_site_policy() {
        let builder = HandlerBuilder::new(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .same_site_policy(SameSite::None);
        assert_eq!(builder.same_site_policy, SameSite::None);
    }

    #[test]
    fn test_handler_builder_cookie_domain() {
        let builder = HandlerBuilder::new(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .cookie_domain("example.com");
        assert_eq!(builder.cookie_domain, Some("example.com".to_string()));
    }

    #[test]
    fn test_handler_builder_fallback_keys() {
        let builder = HandlerBuilder::new(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .fallback_keys(vec![Key::from(
            b"fallbackfallbackfallbackfallbackfallbackfallbackfallbackfallback" as &[u8],
        )]);
        assert_eq!(builder.fallback_keys.len(), 1);
    }

    #[test]
    fn test_handler_builder_add_fallback_key() {
        let builder = HandlerBuilder::new(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .add_fallback_key(Key::from(
            b"fallbackfallbackfallbackfallbackfallbackfallbackfallbackfallback" as &[u8],
        ))
        .add_fallback_key(Key::from(
            b"anotherkeyanotherkeyanotherkeyanotherkeyanotherkeyanotherkeyanot" as &[u8],
        ));
        assert_eq!(builder.fallback_keys.len(), 2);
    }

    #[test]
    fn test_handler_builder_build() {
        let handler = HandlerBuilder::new(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .build()
        .unwrap();
        assert_eq!(handler.cookie_path, "/");
        assert_eq!(handler.cookie_name, "salvo.session.id");
    }

    #[test]
    fn test_handler_builder_debug() {
        let builder = HandlerBuilder::new(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        );
        let debug_str = format!("{:?}", builder);
        assert!(debug_str.contains("HandlerBuilder"));
        assert!(debug_str.contains("cookie_path"));
        assert!(debug_str.contains("cookie_name"));
    }

    #[test]
    fn test_handler_builder_chain() {
        let handler = HandlerBuilder::new(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .cookie_path("/app")
        .cookie_name("app_session")
        .cookie_domain("app.example.com")
        .session_ttl(Some(Duration::from_secs(7200)))
        .save_unchanged(false)
        .same_site_policy(SameSite::Strict)
        .build()
        .unwrap();

        assert_eq!(handler.cookie_path, "/app");
        assert_eq!(handler.cookie_name, "app_session");
        assert_eq!(handler.cookie_domain, Some("app.example.com".to_string()));
        assert_eq!(handler.session_ttl, Some(Duration::from_secs(7200)));
        assert!(!handler.save_unchanged);
        assert_eq!(handler.same_site_policy, SameSite::Strict);
    }

    // Tests for SessionHandler
    #[test]
    fn test_session_handler_builder() {
        let handler = SessionHandler::builder(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .build()
        .unwrap();
        assert_eq!(handler.cookie_name, "salvo.session.id");
    }

    #[test]
    fn test_session_handler_debug() {
        let handler = SessionHandler::builder(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .build()
        .unwrap();
        let debug_str = format!("{:?}", handler);
        assert!(debug_str.contains("SessionHandler"));
        assert!(debug_str.contains("cookie_path"));
    }

    // Tests for SessionDepotExt
    #[test]
    fn test_depot_set_session() {
        let mut depot = Depot::new();
        let session = Session::new();
        depot.set_session(session);
        assert!(depot.session().is_some());
    }

    #[test]
    fn test_depot_take_session() {
        let mut depot = Depot::new();
        let session = Session::new();
        depot.set_session(session);
        let taken = depot.take_session();
        assert!(taken.is_some());
        assert!(depot.session().is_none());
    }

    #[test]
    fn test_depot_session() {
        let mut depot = Depot::new();
        assert!(depot.session().is_none());

        depot.set_session(Session::new());
        assert!(depot.session().is_some());
    }

    #[test]
    fn test_depot_session_mut() {
        let mut depot = Depot::new();
        depot.set_session(Session::new());

        if let Some(session) = depot.session_mut() {
            session.insert("key", "value").unwrap();
        }

        if let Some(session) = depot.session() {
            assert_eq!(session.get::<String>("key"), Some("value".to_string()));
        }
    }

    // Tests for session with destroyed state
    #[tokio::test]
    async fn test_session_destroy() {
        #[handler]
        pub async fn destroy_session(depot: &mut Depot, res: &mut Response) {
            if let Some(session) = depot.session_mut() {
                session.destroy();
            }
            res.render("destroyed");
        }

        let session_handler = SessionHandler::builder(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .build()
        .unwrap();

        let router = Router::new()
            .hoop(session_handler)
            .push(Router::with_path("destroy").get(destroy_session));
        let service = Service::new(router);

        let response = TestClient::get("http://127.0.0.1:8698/destroy")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
    }

    // Tests for session with save_unchanged = false
    #[tokio::test]
    async fn test_session_save_unchanged_false() {
        #[handler]
        pub async fn no_change(depot: &mut Depot, res: &mut Response) {
            // Access session but don't modify it
            let _ = depot.session();
            res.render("no change");
        }

        let session_handler = SessionHandler::builder(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .save_unchanged(false)
        .build()
        .unwrap();

        let router = Router::new()
            .hoop(session_handler)
            .push(Router::with_path("nochange").get(no_change));
        let service = Service::new(router);

        let response = TestClient::get("http://127.0.0.1:8698/nochange")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        // When save_unchanged is false and no data is modified, no cookie should be set
        // for a new session (unless there's existing session data)
    }

    // Tests for session data persistence
    #[tokio::test]
    async fn test_session_data_persistence() {
        #[handler]
        pub async fn set_data(depot: &mut Depot, res: &mut Response) {
            if let Some(session) = depot.session_mut() {
                session.insert("counter", 1).unwrap();
            }
            res.render("set");
        }

        #[handler]
        pub async fn get_data(depot: &mut Depot, res: &mut Response) {
            let counter = if let Some(session) = depot.session() {
                session.get::<i32>("counter").unwrap_or(0)
            } else {
                0
            };
            res.render(format!("{}", counter));
        }

        let session_handler = SessionHandler::builder(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .build()
        .unwrap();

        let router = Router::new()
            .hoop(session_handler)
            .push(Router::with_path("set").get(set_data))
            .push(Router::with_path("get").get(get_data));
        let service = Service::new(router);

        // Set data
        let response = TestClient::get("http://127.0.0.1:8698/set")
            .send(&service)
            .await;
        let cookie = response.headers().get(SET_COOKIE).unwrap();

        // Get data with same session
        let mut response = TestClient::get("http://127.0.0.1:8698/get")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert_eq!(response.take_string().await.unwrap(), "1");
    }

    // Test for SESSION_KEY constant
    #[test]
    fn test_session_key_constant() {
        assert_eq!(SESSION_KEY, "::salvo::session");
    }

    // Test for BASE64_DIGEST_LEN constant
    #[test]
    fn test_base64_digest_len() {
        assert_eq!(BASE64_DIGEST_LEN, 44);
    }
}
