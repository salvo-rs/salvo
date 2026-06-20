use std::fmt::{self, Debug, Formatter};

use salvo_core::http::SecureCookiePolicy;
use salvo_core::http::cookie::time::Duration;
use salvo_core::http::cookie::{Cookie, CookieJar, Key, SameSite};
use salvo_core::{Depot, Request, Response};

use super::{Flash, FlashHandler, FlashStore};

/// CookieStore is a `FlashStore` implementation that stores the flash messages in a cookie.
#[non_exhaustive]
pub struct CookieStore {
    /// The cookie max age.
    pub max_age: Duration,
    /// The cookie same site.
    pub same_site: SameSite,
    /// Policy for deciding whether flash cookies include `Secure`.
    pub secure_cookie_policy: SecureCookiePolicy,
    /// The cookie http only.
    pub http_only: bool,
    /// The cookie path.
    pub path: String,
    /// The cookie name.
    pub name: String,
    key: Key,
}
impl Debug for CookieStore {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("CookieStore")
            .field("max_age", &self.max_age)
            .field("same_site", &self.same_site)
            .field("secure_cookie_policy", &self.secure_cookie_policy)
            .field("http_only", &self.http_only)
            .field("path", &self.path)
            .field("name", &self.name)
            .field("key", &"<redacted>")
            .finish()
    }
}
impl Default for CookieStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CookieStore {
    /// Create a new `CookieStore`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_age: Duration::seconds(60),
            same_site: SameSite::Lax,
            secure_cookie_policy: SecureCookiePolicy::AutoFromScheme,
            http_only: true,
            path: "/".into(),
            name: "salvo.flash".into(),
            key: Key::generate(),
        }
    }

    /// Sets cookie name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Sets cookie max_age.
    #[must_use]
    pub fn max_age(mut self, max_age: Duration) -> Self {
        self.max_age = max_age;
        self
    }

    /// Sets cookie same site.
    #[must_use]
    pub fn same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = same_site;
        self
    }

    /// Forces the `Secure` attribute for flash cookies.
    ///
    /// By default this is detected from the request URI scheme. Use this option in
    /// production when TLS is terminated before the Salvo application.
    #[must_use]
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure_cookie_policy = SecureCookiePolicy::from_bool(secure);
        self
    }

    /// Sets the policy used to decide whether flash cookies include `Secure`.
    ///
    /// The default policy is [`SecureCookiePolicy::AutoFromScheme`]. `SameSite::None`
    /// flash cookies are always sent with `Secure`.
    #[must_use]
    pub fn secure_cookie_policy(mut self, policy: SecureCookiePolicy) -> Self {
        self.secure_cookie_policy = policy;
        self
    }

    /// Sets cookie http only.
    #[must_use]
    pub fn http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }

    /// Sets cookie path.
    #[must_use]
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Sets the key used to sign flash cookies.
    ///
    /// Use a stable key shared by all application instances when flash messages
    /// must survive restarts or load-balancing across multiple processes.
    #[must_use]
    pub fn key(mut self, key: Key) -> Self {
        self.key = key;
        self
    }

    /// Converts this store into a `FlashHandler`.
    #[must_use]
    pub fn into_handler(self) -> FlashHandler<Self> {
        FlashHandler::new(self)
    }

    fn sign_cookie(&self, cookie: Cookie<'static>) -> Cookie<'static> {
        let mut jar = CookieJar::new();
        jar.signed_mut(&self.key).add(cookie);
        jar.get(&self.name)
            .cloned()
            .expect("signed cookie should be present in jar")
    }

    fn secure_for_request(&self, req: &Request) -> bool {
        self.same_site == SameSite::None || self.secure_cookie_policy.is_secure(req)
    }

    fn clear_cookie(&self, secure: bool) -> Cookie<'static> {
        Cookie::build((self.name.clone(), ""))
            .max_age(Duration::seconds(0))
            .same_site(self.same_site)
            .secure(secure)
            .http_only(self.http_only)
            .path(self.path.clone())
            .build()
    }
}
impl FlashStore for CookieStore {
    async fn load_flash(&self, req: &mut Request, _depot: &mut Depot) -> Option<Flash> {
        match req.cookies().signed(&self.key).get(&self.name) {
            None => None,
            Some(cookie) => match serde_json::from_str(cookie.value()) {
                Ok(flash) => Some(flash),
                Err(e) => {
                    tracing::error!(error = ?e, "deserialize flash cookie failed");
                    None
                }
            },
        }
    }
    async fn save_flash(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        flash: Flash,
    ) {
        let secure = self.same_site == SameSite::None || self.secure_cookie_policy.is_secure(req);
        let value = serde_json::to_string(&flash).unwrap_or_default();
        let cookie = Cookie::build((self.name.clone(), value))
            .max_age(self.max_age)
            .path(self.path.clone())
            .same_site(self.same_site)
            .secure(secure)
            .http_only(self.http_only)
            .build();
        res.add_cookie(self.sign_cookie(cookie));
    }
    async fn clear_flash(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response) {
        res.add_cookie(self.clear_cookie(self.secure_for_request(req)));
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::test::TestClient;

    use super::*;

    #[test]
    fn test_cookie_store_new() {
        let store = CookieStore::new();
        assert_eq!(store.max_age, Duration::seconds(60));
        assert_eq!(store.same_site, SameSite::Lax);
        assert_eq!(
            store.secure_cookie_policy,
            SecureCookiePolicy::AutoFromScheme
        );
        assert!(store.http_only);
        assert_eq!(store.path, "/");
        assert_eq!(store.name, "salvo.flash");
    }

    #[test]
    fn test_cookie_store_default() {
        let store = CookieStore::default();
        assert_eq!(store.name, "salvo.flash");
        assert_eq!(store.path, "/");
    }

    #[test]
    fn test_cookie_store_name() {
        let store = CookieStore::new().name("custom_flash");
        assert_eq!(store.name, "custom_flash");
    }

    #[test]
    fn test_cookie_store_max_age() {
        let store = CookieStore::new().max_age(Duration::seconds(120));
        assert_eq!(store.max_age, Duration::seconds(120));
    }

    #[test]
    fn test_cookie_store_same_site() {
        let store = CookieStore::new().same_site(SameSite::Strict);
        assert_eq!(store.same_site, SameSite::Strict);
    }

    #[test]
    fn test_cookie_store_same_site_none() {
        let store = CookieStore::new().same_site(SameSite::None);
        assert_eq!(store.same_site, SameSite::None);
    }

    #[test]
    fn test_cookie_store_secure() {
        let store = CookieStore::new().secure(true);
        assert_eq!(store.secure_cookie_policy, SecureCookiePolicy::Always);

        let store = CookieStore::new().secure(false);
        assert_eq!(store.secure_cookie_policy, SecureCookiePolicy::Never);
    }

    #[test]
    fn test_cookie_store_secure_cookie_policy() {
        let store = CookieStore::new().secure_cookie_policy(SecureCookiePolicy::Never);
        assert_eq!(store.secure_cookie_policy, SecureCookiePolicy::Never);
    }

    #[test]
    fn test_cookie_store_http_only() {
        let store = CookieStore::new().http_only(false);
        assert!(!store.http_only);
    }

    #[test]
    fn test_cookie_store_path() {
        let store = CookieStore::new().path("/app");
        assert_eq!(store.path, "/app");
    }

    #[test]
    fn test_cookie_store_into_handler() {
        let store = CookieStore::new();
        let handler = store.into_handler();
        assert!(handler.minimum_level.is_none());
    }

    #[test]
    fn test_cookie_store_chain_config() {
        let store = CookieStore::new()
            .name("my_flash")
            .max_age(Duration::seconds(300))
            .same_site(SameSite::Strict)
            .secure(true)
            .http_only(false)
            .path("/dashboard");

        assert_eq!(store.name, "my_flash");
        assert_eq!(store.max_age, Duration::seconds(300));
        assert_eq!(store.same_site, SameSite::Strict);
        assert_eq!(store.secure_cookie_policy, SecureCookiePolicy::Always);
        assert!(!store.http_only);
        assert_eq!(store.path, "/dashboard");
    }

    #[test]
    fn test_cookie_store_debug() {
        let store = CookieStore::new();
        let debug_str = format!("{store:?}");
        assert!(debug_str.contains("CookieStore"));
        assert!(debug_str.contains("max_age"));
        assert!(debug_str.contains("same_site"));
        assert!(debug_str.contains("http_only"));
        assert!(debug_str.contains("path"));
        assert!(debug_str.contains("name"));
    }

    #[tokio::test]
    async fn test_cookie_store_loads_signed_flash() {
        let store = CookieStore::new().key(Key::generate());
        let mut req = Request::new();
        let mut depot = Depot::new();
        let mut res = Response::new();
        let mut flash = Flash::default();
        flash.success("saved");

        store
            .save_flash(&mut req, &mut depot, &mut res, flash)
            .await;

        let cookie = res.cookie(&store.name).unwrap().clone();
        let mut next_req = Request::new();
        next_req.cookies_mut().add(cookie);

        let loaded = store.load_flash(&mut next_req, &mut depot).await.unwrap();
        assert_eq!(loaded.0.len(), 1);
        assert_eq!(loaded.0[0].value, "saved");
    }

    #[tokio::test]
    async fn test_cookie_store_infers_secure_from_https_scheme() {
        let store = CookieStore::new().key(Key::generate());
        let mut req = TestClient::get("https://example.com/").build();
        let mut depot = Depot::new();
        let mut res = Response::new();
        let mut flash = Flash::default();
        flash.success("saved");

        store
            .save_flash(&mut req, &mut depot, &mut res, flash)
            .await;

        let cookie = res.cookie(&store.name).unwrap();
        assert_eq!(cookie.secure(), Some(true));
    }

    #[tokio::test]
    async fn test_cookie_store_secure_policy_can_force_secure() {
        let store = CookieStore::new()
            .key(Key::generate())
            .secure_cookie_policy(SecureCookiePolicy::Always);
        let mut req = TestClient::get("http://example.com/").build();
        let mut depot = Depot::new();
        let mut res = Response::new();
        let mut flash = Flash::default();
        flash.success("saved");

        store
            .save_flash(&mut req, &mut depot, &mut res, flash)
            .await;

        let cookie = res.cookie(&store.name).unwrap();
        assert_eq!(cookie.secure(), Some(true));
    }

    #[tokio::test]
    async fn test_cookie_store_secure_policy_can_disable_https_secure() {
        let store = CookieStore::new()
            .key(Key::generate())
            .secure_cookie_policy(SecureCookiePolicy::Never);
        let mut req = TestClient::get("https://example.com/").build();
        let mut depot = Depot::new();
        let mut res = Response::new();
        let mut flash = Flash::default();
        flash.success("saved");

        store
            .save_flash(&mut req, &mut depot, &mut res, flash)
            .await;

        let cookie = res.cookie(&store.name).unwrap();
        assert_eq!(cookie.secure(), Some(false));
    }

    #[tokio::test]
    async fn test_cookie_store_same_site_none_forces_secure() {
        let store = CookieStore::new()
            .key(Key::generate())
            .same_site(SameSite::None)
            .secure_cookie_policy(SecureCookiePolicy::Never);
        let mut req = TestClient::get("http://example.com/").build();
        let mut depot = Depot::new();
        let mut res = Response::new();
        let mut flash = Flash::default();
        flash.success("saved");

        store
            .save_flash(&mut req, &mut depot, &mut res, flash)
            .await;

        let cookie = res.cookie(&store.name).unwrap();
        assert_eq!(cookie.same_site(), Some(SameSite::None));
        assert_eq!(cookie.secure(), Some(true));
    }

    #[tokio::test]
    async fn test_cookie_store_clear_same_site_none_uses_secure() {
        let store = CookieStore::new().same_site(SameSite::None);
        let mut req = TestClient::get("http://example.com/").build();
        let mut depot = Depot::new();
        let mut res = Response::new();

        store.clear_flash(&mut req, &mut depot, &mut res).await;

        let cookie = res.cookie(&store.name).unwrap();
        assert_eq!(cookie.max_age(), Some(Duration::seconds(0)));
        assert_eq!(cookie.same_site(), Some(SameSite::None));
        assert_eq!(cookie.secure(), Some(true));
    }

    #[tokio::test]
    async fn test_cookie_store_clear_infers_secure_from_https_scheme() {
        let store = CookieStore::new().name("__Secure-salvo.flash");
        let mut req = TestClient::get("https://example.com/").build();
        let mut depot = Depot::new();
        let mut res = Response::new();

        store.clear_flash(&mut req, &mut depot, &mut res).await;

        let cookie = res.cookie(&store.name).unwrap();
        assert_eq!(cookie.max_age(), Some(Duration::seconds(0)));
        assert_eq!(cookie.secure(), Some(true));
    }

    #[tokio::test]
    async fn test_cookie_store_clear_keeps_http_auto_policy_insecure() {
        let store = CookieStore::new();
        let mut req = TestClient::get("http://example.com/").build();
        let mut depot = Depot::new();
        let mut res = Response::new();

        store.clear_flash(&mut req, &mut depot, &mut res).await;

        let cookie = res.cookie(&store.name).unwrap();
        assert_eq!(cookie.max_age(), Some(Duration::seconds(0)));
        assert_eq!(cookie.secure(), Some(false));
    }

    #[tokio::test]
    async fn test_cookie_store_rejects_unsigned_flash_cookie() {
        let store = CookieStore::new().key(Key::generate());
        let mut req = Request::new();
        let mut depot = Depot::new();
        let mut flash = Flash::default();
        flash.success("forged");
        req.cookies_mut().add(Cookie::new(
            store.name.clone(),
            serde_json::to_string(&flash).unwrap(),
        ));

        assert!(store.load_flash(&mut req, &mut depot).await.is_none());
    }
}
