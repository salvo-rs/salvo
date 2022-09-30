//! The csrf lib of Savlo web server framework. Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod finder;

pub use finder::{CsrfTokenFinder, FormFinder, HeaderFinder, JsonFinder, QueryFinder};

use base64::URL_SAFE_NO_PAD;
use rand::distributions::Standard;
use rand::Rng;
use salvo_core::http::{Method, StatusCode};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler, Request, Response};

#[macro_use]
mod cfg;

cfg_feature! {
    #![feature = "cookie-store"]

    mod cookie_store;
    pub use cookie_store::CookieStore;

    /// Helper function to create a `CookieStore`.
    pub fn cookie_store<>() -> CookieStore {
        CookieStore::new()
    }
}

cfg_feature! {
    #![feature = "session-store"]

    mod session_store;
    pub use session_store::SessionStore;

    /// Helper function to create a `SessionStore`.
    pub fn session_store() -> SessionStore {
        SessionStore::new()
    }
}
cfg_feature! {
    #![feature = "hmac-cipher"]

    mod hmac_cipher;
    pub use hmac_cipher::HmacCipher;
}
cfg_feature! {
    #![feature = "bcrypt-cipher"]

    mod bcrypt_cipher;
    pub use bcrypt_cipher::BcryptCipher;

    /// Helper function to create a `Csrf` use `BcryptCipher`.
    pub fn bcrypt_csrf<S>(store: S, finder: impl CsrfTokenFinder ) -> Csrf<BcryptCipher, S> where S: CsrfStore {
        Csrf::new(BcryptCipher::new(), store, finder)
    }
}

type FilterFn = Box<dyn Fn(&Request) -> bool + Send + Sync>;

/// key used to insert auth decoded data to depot.
pub const CSRF_TOKEN_KEY: &str = "salvo.csrf.token";

fn default_filter(req: &Request) -> bool {
    [Method::POST, Method::PATCH, Method::DELETE, Method::PUT].contains(req.method())
}

/// Store secret.
#[async_trait]
pub trait CsrfStore: Send + Sync + 'static {
    /// Error type for CsrfStore.
    type Error: std::error::Error + Send + Sync + 'static;
    /// Get the secret from the store.
    async fn load_secret(&self, req: &mut Request, depot: &mut Depot) -> Option<Vec<u8>>;
    /// Save the secret from the store.
    async fn save_secret(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        secret: &[u8],
    ) -> Result<(), Self::Error>;
}

/// Generate secret and token and valid token.
pub trait CsrfCipher: Send + Sync + 'static {
    /// Verify token is valid.
    fn verify(&self, secret: &[u8], token: &[u8]) -> bool;
    /// Generate new secret and token.
    fn generate(&self) -> (Vec<u8>, Vec<u8>);

    /// Generate a random bytes.
    fn random_bytes(&self, len: usize) -> Vec<u8> {
        rand::thread_rng().sample_iter(Standard).take(len).collect()
    }
}

/// Extesion for Depot.
pub trait CsrfDepotExt {
    /// Get csrf token reference from depot.
    fn csrf_token(&self) -> Option<&String>;
}

impl CsrfDepotExt for Depot {
    #[inline]
    fn csrf_token(&self) -> Option<&String> {
        self.get(CSRF_TOKEN_KEY)
    }
}

/// Cross-Site Request Forgery (CSRF) protection middleware.
pub struct Csrf<C, S> {
    cipher: C,
    store: S,
    filter: FilterFn,
    finders: Vec<Box<dyn CsrfTokenFinder>>,
    fallback_ciphers: Vec<Box<dyn CsrfCipher>>,
}

impl<C: CsrfCipher, S: CsrfStore> Csrf<C, S> {
    /// Create a new instance.
    #[inline]
    pub fn new(cipher: C, store: S, finder: impl CsrfTokenFinder) -> Self {
        Self {
            cipher,
            store,
            filter: Box::new(default_filter),
            finders: vec![Box::new(finder)],
            fallback_ciphers: vec![],
        }
    }

    /// Add finder to find csrf token.
    #[inline]
    pub fn add_finder(mut self, finder: impl CsrfTokenFinder) -> Self {
        self.finders.push(Box::new(finder));
        self
    }
    /// Add finder to find csrf token.
    #[inline]
    pub fn add_fallabck_cipher(mut self, cipher: impl CsrfCipher) -> Self {
        self.fallback_ciphers.push(Box::new(cipher));
        self
    }

    // /// Clear all finders.
    // #[inline]
    // pub fn clear_finders(mut self) -> Self {
    //     self.finders = vec![];
    //     self
    // }

    // /// Set all finders.
    // #[inline]
    // pub fn with_finders(mut self, finders: Vec<Box<dyn CsrfTokenFinder>>) -> Self {
    //     self.finders = finders;
    //     self
    // }

    async fn find_token(&self, req: &mut Request) -> Option<String> {
        for finder in self.finders.iter() {
            if let Some(given_token) = finder.find_token(req).await {
                return Some(given_token);
            }
        }
        None
    }
}

#[async_trait]
impl<C: CsrfCipher, S: CsrfStore> Handler for Csrf<C, S> {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if (self.filter)(req) {
            if let Some(token) = &self.find_token(req).await {
                tracing::debug!("csrf token: {:?}", token);
                if let Ok(token) = base64::decode_config(token, URL_SAFE_NO_PAD) {
                    if let Some(secret) = self.store.load_secret(req, depot).await {
                        let mut valid = self.cipher.verify(&secret, &token);
                        if !valid && self.fallback_ciphers.is_empty() {
                            tracing::debug!("try to use fallback ciphers to verify CSRF token");
                            for cipher in &self.fallback_ciphers {
                                if cipher.verify(&secret, &token) {
                                    tracing::debug!("fallback cipher verify CSRF token success");
                                    valid = true;
                                    break;
                                }
                            }
                        } else {
                            tracing::debug!("cipher verify CSRF token success");
                        }
                        if !valid {
                            tracing::debug!("rejecting request due to invalid or expired CSRF token");
                            res.set_status_code(StatusCode::FORBIDDEN);
                            ctrl.skip_rest();
                            return;
                        }
                    } else {
                        tracing::debug!("rejecting request due to missing CSRF token",);
                        res.set_status_code(StatusCode::FORBIDDEN);
                        ctrl.skip_rest();
                        return;
                    }
                } else {
                    tracing::debug!("rejecting request due to decode token failed",);
                    res.set_status_code(StatusCode::FORBIDDEN);
                    ctrl.skip_rest();
                    return;
                }
            } else {
                tracing::debug!("rejecting request due to missing CSRF cookie",);
                res.set_status_code(StatusCode::FORBIDDEN);
                ctrl.skip_rest();
                return;
            }
        }
        let (origin, encrypted) = self.cipher.generate();
        if let Err(e) = self.store.save_secret(req, depot, res, &origin).await {
            tracing::error!(error = ?e, "salve csrf token failed");
        }
        let encrypted = base64::encode_config(&encrypted, URL_SAFE_NO_PAD);
        tracing::debug!("new token: {:?}", encrypted);
        depot.insert(CSRF_TOKEN_KEY, encrypted);
        ctrl.call_next(req, depot, res).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    #[handler]
    async fn get_index(depot: &mut Depot) -> String {
        depot.csrf_token().unwrap().to_owned()
    }
    #[handler]
    async fn post_index() -> &'static str {
        "POST"
    }

    #[tokio::test]
    async fn test_exposes_csrf_request_extensions() {
        let csrf = Csrf::new(BcryptCipher::new(), CookieStore::new(), HeaderFinder::new());
        let router = Router::new().hoop(csrf).get(get_index);
        let res = TestClient::get("http://127.0.0.1:7979").send(router).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_adds_csrf_cookie_sets_request_token() {
        let csrf = Csrf::new(BcryptCipher::new(), CookieStore::new(), HeaderFinder::new());
        let router = Router::new().hoop(csrf).get(get_index);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(router).await;

        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
        assert_ne!(res.take_string().await.unwrap(), "");
        assert_ne!(res.cookie("salvo.csrf.secret"), None);
    }

    #[tokio::test]
    async fn test_validates_token_in_header() {
        let csrf = Csrf::new(BcryptCipher::new(), CookieStore::new(), HeaderFinder::new());
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.csrf.secret").unwrap();

        let res = TestClient::post("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::FORBIDDEN);

        let mut res = TestClient::post("http://127.0.0.1:7979")
            .add_header("x-csrf-token", csrf_token, true)
            .add_header("cookie", cookie.to_string(), true)
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
        assert_eq!(res.take_string().await.unwrap(), "POST");
    }

    #[tokio::test]
    async fn test_validates_token_in_custom_header() {
        let csrf = Csrf::new(
            BcryptCipher::new(),
            CookieStore::new(),
            HeaderFinder::new().with_header_name("x-mycsrf-header"),
        );
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.csrf.secret").unwrap();

        let res = TestClient::post("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::FORBIDDEN);

        let mut res = TestClient::post("http://127.0.0.1:7979")
            .add_header("x-mycsrf-header", csrf_token, true)
            .add_header("cookie", cookie.to_string(), true)
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
        assert_eq!(res.take_string().await.unwrap(), "POST");
    }

    #[tokio::test]
    async fn test_validates_token_in_query() {
        let csrf = Csrf::new(BcryptCipher::new(), CookieStore::new(), QueryFinder::new());
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.csrf.secret").unwrap();

        let res = TestClient::post("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::FORBIDDEN);

        let mut res = TestClient::post(format!("http://127.0.0.1:7979?a=1&csrf-token={}&b=2", csrf_token))
            .add_header("cookie", cookie.to_string(), true)
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
        assert_eq!(res.take_string().await.unwrap(), "POST");
    }
    #[cfg(feature = "hmac-cipher")]
    #[tokio::test]
    async fn test_validates_token_in_alternate_query() {
        let csrf = Csrf::new(
            HmacCipher::new(*b"01234567012345670123456701234567"),
            CookieStore::new(),
            QueryFinder::new().with_query_name("my-csrf-token"),
        );
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.csrf.secret").unwrap();

        let res = TestClient::post("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::FORBIDDEN);

        let mut res = TestClient::post(format!("http://127.0.0.1:7979?a=1&my-csrf-token={}&b=2", csrf_token))
            .add_header("cookie", cookie.to_string(), true)
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
        assert_eq!(res.take_string().await.unwrap(), "POST");
    }

    #[cfg(feature = "hmac-cipher")]
    #[tokio::test]
    async fn test_validates_token_in_form() {
        let csrf = Csrf::new(
            HmacCipher::new(*b"01234567012345670123456701234567"),
            CookieStore::new(),
            QueryFinder::new().with_query_name("my-csrf-token"),
        );
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.csrf.secret").unwrap();

        let res = TestClient::post("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::FORBIDDEN);

        let mut res = TestClient::post("http://127.0.0.1:7979")
            .add_header("cookie", cookie.to_string(), true)
            .form(&[("a", "1"), ("csrf-token", &*csrf_token), ("b", "2")])
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
        assert_eq!(res.take_string().await.unwrap(), "POST");
    }
    #[tokio::test]
    async fn test_validates_token_in_alternate_form() {
        let csrf = Csrf::new(
            BcryptCipher::new(),
            CookieStore::new(),
            FormFinder::new().with_field_name("my-csrf-token"),
        );
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.csrf.secret").unwrap();

        let res = TestClient::post("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::FORBIDDEN);
        let mut res = TestClient::post("http://127.0.0.1:7979")
            .add_header("cookie", cookie.to_string(), true)
            .form(&[("a", "1"), ("my-csrf-token", &*csrf_token), ("b", "2")])
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
        assert_eq!(res.take_string().await.unwrap(), "POST");
    }

    #[tokio::test]
    async fn test_rejects_short_token() {
        let csrf = Csrf::new(BcryptCipher::new(), CookieStore::new(), HeaderFinder::new());
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let cookie = res.cookie("salvo.csrf.secret").unwrap();

        let res = TestClient::post("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::FORBIDDEN);

        let res = TestClient::post("http://127.0.0.1:7979")
            .add_header("x-csrf-token", "aGVsbG8=", true)
            .add_header("cookie", cookie.to_string(), true)
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_rejects_invalid_base64_token() {
        let csrf = Csrf::new(BcryptCipher::new(), CookieStore::new(), HeaderFinder::new());
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let cookie = res.cookie("salvo.csrf.secret").unwrap();

        let res = TestClient::post("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::FORBIDDEN);

        let res = TestClient::post("http://127.0.0.1:7979")
            .add_header("x-csrf-token", "aGVsbG8", true)
            .add_header("cookie", cookie.to_string(), true)
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_rejects_mismatched_token() {
        let csrf = Csrf::new(BcryptCipher::new(), CookieStore::new(), HeaderFinder::new());
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
        let csrf_token = res.take_string().await.unwrap();

        let res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
        let cookie = res.cookie("salvo.csrf.secret").unwrap();

        let res = TestClient::post("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::FORBIDDEN);

        let res = TestClient::post("http://127.0.0.1:7979")
            .add_header("x-csrf-token", csrf_token, true)
            .add_header("cookie", cookie.to_string(), true)
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::FORBIDDEN);
    }
}
