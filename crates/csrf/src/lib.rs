//! CSRF middleware for Salvo web framework.
//!
//! CSRF middleware for Salvo that provides CSRF (Cross-Site Request Forgery) protection.
//!
//! Data can be saved in Cookies via [`CookieStore`](struct.CookieStore.html) or in session
//! via [`SessionStore`](struct.SessionStore.html). [`SessionStore`](struct.SessionStore.html) need to work with `salvo-session` crate.
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::error::Error as StdError;

mod finder;

pub use finder::{CsrfTokenFinder, FormFinder, HeaderFinder, JsonFinder};

use rand::Rng;
use rand::distr::StandardUniform;
use salvo_core::handler::Skipper;
use salvo_core::http::{Method, StatusCode};
use salvo_core::{Depot, FlowCtrl, Handler, Request, Response, async_trait};

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
    #![feature = "bcrypt-cipher"]

    mod bcrypt_cipher;
    pub use bcrypt_cipher::BcryptCipher;

    /// Helper function to create a `Csrf` use `BcryptCipher`.
    pub fn bcrypt_csrf<S>(store: S, finder: impl CsrfTokenFinder ) -> Csrf<BcryptCipher, S> where S: CsrfStore {
        Csrf::new(BcryptCipher::new(), store, finder)
    }
}
cfg_feature! {
    #![all(feature = "bcrypt-cipher", feature = "cookie-store")]
    /// Helper function to create a `Csrf` use `BcryptCipher` and `CookieStore`.
    pub fn bcrypt_cookie_csrf(finder: impl CsrfTokenFinder ) -> Csrf<BcryptCipher, CookieStore> {
        Csrf::new(BcryptCipher::new(), CookieStore::new(), finder)
    }
}
cfg_feature! {
    #![all(feature = "bcrypt-cipher", feature = "session-store")]
    /// Helper function to create a `Csrf` use `BcryptCipher` and `SessionStore`.
    pub fn bcrypt_session_csrf(finder: impl CsrfTokenFinder ) -> Csrf<BcryptCipher, SessionStore> {
        Csrf::new(BcryptCipher::new(), SessionStore::new(), finder)
    }
}

cfg_feature! {
    #![feature = "hmac-cipher"]

    mod hmac_cipher;
    pub use hmac_cipher::HmacCipher;

    /// Helper function to create a `Csrf` use `HmacCipher`.
    pub fn hmac_csrf<S>(hmac_key: [u8; 32], store: S, finder: impl CsrfTokenFinder ) -> Csrf<HmacCipher, S> where S: CsrfStore {
        Csrf::new(HmacCipher::new(hmac_key), store, finder)
    }
}
cfg_feature! {
    #![all(feature = "hmac-cipher", feature = "cookie-store")]
    /// Helper function to create a `Csrf` use `HmacCipher` and `CookieStore`.
    pub fn hmac_cookie_csrf(aead_key: [u8; 32], finder: impl CsrfTokenFinder ) -> Csrf<HmacCipher, CookieStore> {
        Csrf::new(HmacCipher::new(aead_key), CookieStore::new(), finder)
    }
}
cfg_feature! {
    #![all(feature = "hmac-cipher", feature = "session-store")]
    /// Helper function to create a `Csrf` use `HmacCipher` and `SessionStore`.
    pub fn hmac_session_csrf(aead_key: [u8; 32], finder: impl CsrfTokenFinder ) -> Csrf<HmacCipher, SessionStore> {
        Csrf::new(HmacCipher::new(aead_key), SessionStore::new(), finder)
    }
}

cfg_feature! {
    #![feature = "aes-gcm-cipher"]

    mod aes_gcm_cipher;
    pub use aes_gcm_cipher::AesGcmCipher;

    /// Helper function to create a `Csrf` use `AesGcmCipher`.
    pub fn aes_gcm_csrf<S>(aead_key: [u8; 32], store: S, finder: impl CsrfTokenFinder ) -> Csrf<AesGcmCipher, S> where S: CsrfStore {
        Csrf::new(AesGcmCipher::new(aead_key), store, finder)
    }
}
cfg_feature! {
    #![all(feature = "aes-gcm-cipher", feature = "cookie-store")]
    /// Helper function to create a `Csrf` use `AesGcmCipher` and `CookieStore`.
    pub fn aes_gcm_cookie_csrf(aead_key: [u8; 32], finder: impl CsrfTokenFinder ) -> Csrf<AesGcmCipher, CookieStore> {
        Csrf::new(AesGcmCipher::new(aead_key), CookieStore::new(), finder)
    }
}
cfg_feature! {
    #![all(feature = "aes-gcm-cipher", feature = "session-store")]
    /// Helper function to create a `Csrf` use `AesGcmCipher` and `SessionStore`.
    pub fn aes_gcm_session_csrf(aead_key: [u8; 32], finder: impl CsrfTokenFinder ) -> Csrf<AesGcmCipher, SessionStore> {
        Csrf::new(AesGcmCipher::new(aead_key), SessionStore::new(), finder)
    }
}

cfg_feature! {
    #![feature = "ccp-cipher"]

    mod ccp_cipher;
    pub use ccp_cipher::CcpCipher;

    /// Helper function to create a `Csrf` use `CcpCipher`.
    pub fn ccp_csrf<S>(aead_key: [u8; 32], store: S, finder: impl CsrfTokenFinder ) -> Csrf<CcpCipher, S> where S: CsrfStore {
        Csrf::new(CcpCipher::new(aead_key), store, finder)
    }
}
cfg_feature! {
    #![all(feature = "ccp-cipher", feature = "cookie-store")]
    /// Helper function to create a `Csrf` use `CcpCipher` and `CookieStore`.
    pub fn ccp_cookie_csrf(aead_key: [u8; 32], finder: impl CsrfTokenFinder ) -> Csrf<CcpCipher, CookieStore> {
        Csrf::new(CcpCipher::new(aead_key), CookieStore::new(), finder)
    }
}
cfg_feature! {
    #![all(feature = "ccp-cipher", feature = "session-store")]
    /// Helper function to create a `Csrf` use `CcpCipher` and `SessionStore`.
    pub fn ccp_session_csrf(aead_key: [u8; 32], finder: impl CsrfTokenFinder ) -> Csrf<CcpCipher, SessionStore> {
        Csrf::new(CcpCipher::new(aead_key), SessionStore::new(), finder)
    }
}

/// key used to insert auth decoded data to depot.
pub const CSRF_TOKEN_KEY: &str = "salvo.csrf.token";

fn default_skipper(req: &mut Request, _depot: &Depot) -> bool {
    ![Method::POST, Method::PATCH, Method::DELETE, Method::PUT].contains(req.method())
}

/// Store proof.
pub trait CsrfStore: Send + Sync + 'static {
    /// Error type for CsrfStore.
    type Error: StdError + Send + Sync + 'static;
    /// Get the proof from the store.
    fn load<C: CsrfCipher>(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        cipher: &C,
    ) -> impl Future<Output = Option<(String, String)>> + Send;
    /// Save the proof from the store.
    fn save(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        token: &str,
        proof: &str,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// Generate token and proof and valid token.
pub trait CsrfCipher: Send + Sync + 'static {
    /// Verify token is valid.
    fn verify(&self, token: &str, proof: &str) -> bool;
    /// Generate new token and proof.
    fn generate(&self) -> (String, String);

    /// Generate a random bytes.
    fn random_bytes(&self, len: usize) -> Vec<u8> {
        rand::rng().sample_iter(StandardUniform).take(len).collect()
    }
}

/// Extesion for Depot.
pub trait CsrfDepotExt {
    /// Get csrf token reference from depot.
    fn csrf_token(&self) -> Option<&str>;
}

impl CsrfDepotExt for Depot {
    #[inline]
    fn csrf_token(&self) -> Option<&str> {
        self.get::<String>(CSRF_TOKEN_KEY).map(|v| &**v).ok()
    }
}

/// Cross-Site Request Forgery (CSRF) protection middleware.
pub struct Csrf<C, S> {
    cipher: C,
    store: S,
    skipper: Box<dyn Skipper>,
    finders: Vec<Box<dyn CsrfTokenFinder>>,
}

impl<C: CsrfCipher, S: CsrfStore> Csrf<C, S> {
    /// Create a new instance.
    #[inline]
    pub fn new(cipher: C, store: S, finder: impl CsrfTokenFinder) -> Self {
        Self {
            cipher,
            store,
            skipper: Box::new(default_skipper),
            finders: vec![Box::new(finder)],
        }
    }

    /// Add finder to find csrf token.
    #[inline]
    pub fn add_finder(mut self, finder: impl CsrfTokenFinder) -> Self {
        self.finders.push(Box::new(finder));
        self
    }

    // /// Clear all finders.
    // #[inline]
    // pub fn clear_finders(mut self) -> Self {
    //     self.finders = vec![];
    //     self
    // }

    // /// Sets all finders.
    // #[inline]
    // pub fn with_finders(mut self, finders: Vec<Box<dyn CsrfTokenFinder>>) -> Self {
    //     self.finders = finders;
    //     self
    // }

    async fn find_token(&self, req: &mut Request) -> Option<String> {
        for finder in self.finders.iter() {
            if let Some(token) = finder.find_token(req).await {
                return Some(token);
            }
        }
        None
    }
}

#[async_trait]
impl<C: CsrfCipher, S: CsrfStore> Handler for Csrf<C, S> {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        match self.store.load(req, depot, &self.cipher).await {
            Some((token, proof)) => {
                depot.insert(CSRF_TOKEN_KEY, token);

                if !self.skipper.skipped(req, depot) {
                    if let Some(token) = &self.find_token(req).await {
                        tracing::debug!("csrf token: {token}");
                        if !self.cipher.verify(token, &proof) {
                            tracing::debug!(
                                "rejecting request due to invalid or expired CSRF token"
                            );
                            res.status_code(StatusCode::FORBIDDEN);
                            ctrl.skip_rest();
                            return;
                        } else {
                            tracing::debug!("cipher verify CSRF token success");
                        }
                    } else {
                        tracing::debug!("rejecting request due to missing CSRF token",);
                        res.status_code(StatusCode::FORBIDDEN);
                        ctrl.skip_rest();
                        return;
                    }
                }
                ctrl.call_next(req, depot, res).await;
            }
            None => {
                if !self.skipper.skipped(req, depot) {
                    tracing::debug!("rejecting request due to missing CSRF token",);
                    res.status_code(StatusCode::FORBIDDEN);
                    ctrl.skip_rest();
                } else {
                    let (token, proof) = self.cipher.generate();
                    if let Err(e) = self.store.save(req, depot, res, &token, &proof).await {
                        tracing::error!(error = ?e, "salvo csrf token failed");
                    }
                    tracing::debug!("new token: {:?}", token);
                    depot.insert(CSRF_TOKEN_KEY, token);
                    ctrl.call_next(req, depot, res).await;
                }
            }
        }
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
        let csrf = Csrf::new(
            BcryptCipher::new(),
            CookieStore::new(),
            HeaderFinder::new("x-csrf-token"),
        );
        let router = Router::new().hoop(csrf).get(get_index);
        let res = TestClient::get("http://127.0.0.1:5801").send(router).await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_adds_csrf_cookie_sets_request_token() {
        let csrf = Csrf::new(
            BcryptCipher::new(),
            CookieStore::new(),
            HeaderFinder::new("x-csrf-token"),
        );
        let router = Router::new().hoop(csrf).get(get_index);

        let mut res = TestClient::get("http://127.0.0.1:5801").send(router).await;

        assert_eq!(res.status_code.unwrap(), StatusCode::OK);
        assert_ne!(res.take_string().await.unwrap(), "");
        assert_ne!(res.cookie("salvo.csrf"), None);
    }

    #[tokio::test]
    async fn test_validates_token_in_header() {
        let csrf = Csrf::new(
            BcryptCipher::new(),
            CookieStore::new(),
            HeaderFinder::new("x-csrf-token"),
        );
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.csrf").unwrap();

        let res = TestClient::post("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::FORBIDDEN);

        let mut res = TestClient::post("http://127.0.0.1:5801")
            .add_header("x-csrf-token", csrf_token, true)
            .add_header("cookie", cookie.to_string(), true)
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);
        assert_eq!(res.take_string().await.unwrap(), "POST");
    }

    #[tokio::test]
    async fn test_validates_token_in_custom_header() {
        let csrf = Csrf::new(
            BcryptCipher::new(),
            CookieStore::new(),
            HeaderFinder::new("x-mycsrf-header"),
        );
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.csrf").unwrap();

        let res = TestClient::post("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::FORBIDDEN);

        let mut res = TestClient::post("http://127.0.0.1:5801")
            .add_header("x-mycsrf-header", csrf_token, true)
            .add_header("cookie", cookie.to_string(), true)
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);
        assert_eq!(res.take_string().await.unwrap(), "POST");
    }

    #[tokio::test]
    async fn test_validates_token_in_query() {
        let csrf = Csrf::new(
            BcryptCipher::new(),
            CookieStore::new(),
            HeaderFinder::new("csrf-token"),
        );
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.csrf").unwrap();

        let res = TestClient::post("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::FORBIDDEN);

        let mut res = TestClient::post("http://127.0.0.1:5801?a=1&b=2")
            .add_header("csrf-token", csrf_token, true)
            .add_header("cookie", cookie.to_string(), true)
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);
        assert_eq!(res.take_string().await.unwrap(), "POST");
    }
    #[cfg(feature = "hmac-cipher")]
    #[tokio::test]
    async fn test_validates_token_in_alternate_query() {
        let csrf = Csrf::new(
            HmacCipher::new(*b"01234567012345670123456701234567"),
            CookieStore::new(),
            HeaderFinder::new("my-csrf-token"),
        );
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.csrf").unwrap();

        let res = TestClient::post("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::FORBIDDEN);

        let mut res = TestClient::post("http://127.0.0.1:5801?a=1&b=2")
            .add_header("my-csrf-token", csrf_token, true)
            .add_header("cookie", cookie.to_string(), true)
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);
        assert_eq!(res.take_string().await.unwrap(), "POST");
    }

    #[cfg(feature = "hmac-cipher")]
    #[tokio::test]
    async fn test_validates_token_in_form() {
        let csrf = Csrf::new(
            HmacCipher::new(*b"01234567012345670123456701234567"),
            CookieStore::new(),
            FormFinder::new("csrf-token"),
        );
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.csrf").unwrap();

        let res = TestClient::post("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::FORBIDDEN);

        let mut res = TestClient::post("http://127.0.0.1:5801")
            .add_header("cookie", cookie.to_string(), true)
            .form(&[("a", "1"), ("csrf-token", &*csrf_token), ("b", "2")])
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);
        assert_eq!(res.take_string().await.unwrap(), "POST");
    }
    #[tokio::test]
    async fn test_validates_token_in_alternate_form() {
        let csrf = Csrf::new(
            BcryptCipher::new(),
            CookieStore::new(),
            FormFinder::new("my-csrf-token"),
        );
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.csrf").unwrap();

        let res = TestClient::post("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::FORBIDDEN);
        let mut res = TestClient::post("http://127.0.0.1:5801")
            .add_header("cookie", cookie.to_string(), true)
            .form(&[("a", "1"), ("my-csrf-token", &*csrf_token), ("b", "2")])
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);
        assert_eq!(res.take_string().await.unwrap(), "POST");
    }

    #[tokio::test]
    async fn test_rejects_short_token() {
        let csrf = Csrf::new(
            BcryptCipher::new(),
            CookieStore::new(),
            HeaderFinder::new("x-csrf-token"),
        );
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let res = TestClient::get("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let cookie = res.cookie("salvo.csrf").unwrap();

        let res = TestClient::post("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::FORBIDDEN);

        let res = TestClient::post("http://127.0.0.1:5801")
            .add_header("x-csrf-token", "aGVsbG8=", true)
            .add_header(
                "cookie",
                cookie.to_string().split_once('.').unwrap().0,
                true,
            )
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_rejects_invalid_base64_token() {
        let csrf = Csrf::new(
            BcryptCipher::new(),
            CookieStore::new(),
            HeaderFinder::new("x-csrf-token"),
        );
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let res = TestClient::get("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);

        let cookie = res.cookie("salvo.csrf").unwrap();

        let res = TestClient::post("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::FORBIDDEN);

        let res = TestClient::post("http://127.0.0.1:5801")
            .add_header("x-csrf-token", "aGVsbG8", true)
            .add_header(
                "cookie",
                cookie.to_string().split_once('.').unwrap().0,
                true,
            )
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_rejects_mismatched_token() {
        let csrf = Csrf::new(
            BcryptCipher::new(),
            CookieStore::new(),
            HeaderFinder::new("x-csrf-token"),
        );
        let router = Router::new().hoop(csrf).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);
        let csrf_token = res.take_string().await.unwrap();

        let res = TestClient::get("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::OK);
        let cookie = res.cookie("salvo.csrf").unwrap();

        let res = TestClient::post("http://127.0.0.1:5801")
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::FORBIDDEN);

        let res = TestClient::post("http://127.0.0.1:5801")
            .add_header("x-csrf-token", csrf_token, true)
            .add_header(
                "cookie",
                cookie.to_string().split_once('.').unwrap().0,
                true,
            )
            .send(&service)
            .await;
        assert_eq!(res.status_code.unwrap(), StatusCode::FORBIDDEN);
    }
}
