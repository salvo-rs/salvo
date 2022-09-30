mod finder;
pub use finder::{CsrfTokenFinder, FormFinder, HeaderFinder, JsonFinder, QueryFinder};

use base64::URL_SAFE_NO_PAD;
use rand::Rng;
use rand::distributions::Standard;
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

type FilterFn = Box<dyn Fn(&Request) -> bool + Send + Sync>;

/// key used to insert auth decoded data to depot.
pub const CSRF_TOKEN_KEY: &str = "::salvo_csrf::token";

fn default_filter(req: &Request) -> bool {
    [Method::POST, Method::PATCH, Method::DELETE, Method::PUT].contains(&req.method())
}

#[async_trait]
pub trait CsrfStore: Send + Sync + 'static {
    type Error: std::error::Error + Send + Sync + 'static;
    async fn load_secret(&self, req: &mut Request, depot: &mut Depot) -> Option<Vec<u8>>;
    async fn save_secret(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        secret: &[u8],
    ) -> Result<(), Self::Error>;
}
pub trait CsrfCipher: Send + Sync + 'static {
    fn verify(&self, secret: &[u8], token: &[u8]) -> bool;
    fn generate(&self) -> (Vec<u8>, Vec<u8>);
    fn random_bytes(&self, len: usize) -> Vec<u8> {
        rand::thread_rng().sample_iter(Standard).take(len).collect()
    }
}
pub trait CsrfDepotExt {
    /// get csrf token reference from depot.
    fn csrf_token(&self) -> Option<&String>;
}

impl CsrfDepotExt for Depot {
    #[inline]
    fn csrf_token(&self) -> Option<&String> {
        self.get(CSRF_TOKEN_KEY)
    }
}

/// Cross-Site Request Forgery (CSRF) protection middleware.
pub struct Csrf<S, C> {
    store: S,
    cipher: C,
    filter: FilterFn,
    finders: Vec<Box<dyn CsrfTokenFinder>>,
}

impl<S: CsrfStore, C: CsrfCipher> Csrf<S, C> {
    /// Create a new instance.
    #[inline]
    pub fn new(store: S, cipher: C) -> Self {
        Self {
            store,
            cipher,
            filter: Box::new(default_filter),
            finders: vec![],
        }
    }

    #[inline]
    pub fn add_finder(mut self, finder: impl CsrfTokenFinder) -> Self {
        self.finders.push(Box::new(finder));
        self
    }

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
impl<S: CsrfStore, C: CsrfCipher> Handler for Csrf<S, C> {
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if (self.filter)(req) {
            if let Some(token) = &self.find_token(req).await {
                tracing::debug!("csrf token: {:?}", token);
                if let Ok(token) = base64::decode_config(token, URL_SAFE_NO_PAD) {
                    if let Some(secret) = self.store.load_secret(req, depot).await {
                        if !self.cipher.verify(&secret, &token) {
                            tracing::debug!("rejecting request due to invalid or expired CSRF token");
                            res.set_status_code(StatusCode::FORBIDDEN);
                            ctrl.skip_rest();
                            return;
                        } else {
                            tracing::debug!("verified CSRF token");
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

    const SECRET: [u8; 32] = *b"secrets must be >= 32 bytes long";

    #[handler]
    async fn get_index(depot: &mut Depot) -> String {
        depot.csrf_token().unwrap_or_default().to_owned()
    }
    #[handler]
    async fn post_index() -> &'static str {
        "POST"
    }

    #[tokio::test]
    async fn test_exposes_csrf_request_extensions() {
        let router = Router::new().hoop(Csrf::new(&SECRET)).get(get_index);
        let res = TestClient::get("http://127.0.0.1:7979").send(router).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_adds_csrf_cookie_sets_request_token() {
        let router = Router::new().hoop(Csrf::new(&SECRET)).get(get_index);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(router).await;

        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
        assert_ne!(res.take_string().await.unwrap(), "");
        assert_ne!(res.cookie("salvo.extra.csrf"), None);
    }

    #[tokio::test]
    async fn test_validates_token_in_header() {
        let router = Router::new().hoop(Csrf::new(&SECRET)).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.extra.csrf").unwrap();

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
    async fn test_validates_token_in_alternate_header() {
        let router = Router::new()
            .hoop(Csrf::new(&SECRET).with_header_name(HeaderName::from_static("x-mycsrf-header")))
            .get(get_index)
            .post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.extra.csrf").unwrap();

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
        let router = Router::new().hoop(Csrf::new(&SECRET)).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.extra.csrf").unwrap();

        let res = TestClient::post("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::FORBIDDEN);

        let mut res = TestClient::post(format!("http://127.0.0.1:7979?a=1&csrf-token={}&b=2", csrf_token))
            .add_header("cookie", cookie.to_string(), true)
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
        assert_eq!(res.take_string().await.unwrap(), "POST");
    }
    #[tokio::test]
    async fn test_validates_token_in_alternate_query() {
        let router = Router::new()
            .hoop(Csrf::new(&SECRET).with_query_param("my-csrf-token"))
            .get(get_index)
            .post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.extra.csrf").unwrap();

        let res = TestClient::post("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::FORBIDDEN);

        let mut res = TestClient::post(format!("http://127.0.0.1:7979?a=1&my-csrf-token={}&b=2", csrf_token))
            .add_header("cookie", cookie.to_string(), true)
            .send(&service)
            .await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
        assert_eq!(res.take_string().await.unwrap(), "POST");
    }

    #[tokio::test]
    async fn test_validates_token_in_form() {
        let router = Router::new()
            .hoop(Csrf::new(&SECRET).with_query_param("my-csrf-token"))
            .get(get_index)
            .post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.extra.csrf").unwrap();

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
        let router = Router::new()
            .hoop(Csrf::new(&SECRET).with_form_field("my-csrf-token"))
            .get(get_index)
            .post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let csrf_token = res.take_string().await.unwrap();
        let cookie = res.cookie("salvo.extra.csrf").unwrap();

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
        let router = Router::new().hoop(Csrf::new(&SECRET)).get(get_index).post(post_index);
        let service = Service::new(router);

        let res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let cookie = res.cookie("salvo.extra.csrf").unwrap();

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
        let router = Router::new().hoop(Csrf::new(&SECRET)).get(get_index).post(post_index);
        let service = Service::new(router);

        let res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);

        let cookie = res.cookie("salvo.extra.csrf").unwrap();

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
        let router = Router::new().hoop(Csrf::new(&SECRET)).get(get_index).post(post_index);
        let service = Service::new(router);

        let mut res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
        let csrf_token = res.take_string().await.unwrap();

        let res = TestClient::get("http://127.0.0.1:7979").send(&service).await;
        assert_eq!(res.status_code().unwrap(), StatusCode::OK);
        let cookie = res.cookie("salvo.extra.csrf").unwrap();

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
