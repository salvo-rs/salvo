//! jwt auth middleware

use std::marker::PhantomData;

pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::{decode, Algorithm, DecodingKey, TokenData, Validation};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;

use salvo_core::http::{Method, Request, Response, StatusError};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};

mod finder;
pub use finder::{CookieFinder, FormFinder, HeaderFinder, JwtTokenFinder, QueryFinder};

/// key used to insert auth decoded data to depot.
pub const JWT_AUTH_DATA_KEY: &str = "::salvo::jwt_auth::auth_data";
/// key used to insert auth state data to depot.
pub const JWT_AUTH_STATE_KEY: &str = "::salvo::jwt_auth::auth_state";
/// key used to insert auth token data to depot.
pub const JWT_AUTH_TOKEN_KEY: &str = "::salvo::jwt_auth::auth_token";

static ALL_METHODS: Lazy<Vec<Method>> = Lazy::new(|| {
    vec![
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::HEAD,
        Method::OPTIONS,
        Method::CONNECT,
        Method::PATCH,
        Method::TRACE,
    ]
});

/// JwtAuthState
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum JwtAuthState {
    /// Authorized.
    Authorized,
    /// Unauthorized.
    Unauthorized,
    /// Forbidden.
    Forbidden,
}
/// JwtAuthDepotExt
pub trait JwtAuthDepotExt {
    /// get jwt auth token reference from depot.
    fn jwt_auth_token(&self) -> Option<&String>;
    /// get jwt auth decoded data from depot.
    fn jwt_auth_data<C>(&self) -> Option<&TokenData<C>>
    where
        C: DeserializeOwned + Send + Sync + 'static;
    /// get jwt auth state from depot.
    fn jwt_auth_state(&self) -> JwtAuthState;
}

impl JwtAuthDepotExt for Depot {
    #[inline]
    fn jwt_auth_token(&self) -> Option<&String> {
        self.get(JWT_AUTH_TOKEN_KEY)
    }

    #[inline]
    fn jwt_auth_data<C>(&self) -> Option<&TokenData<C>>
    where
        C: DeserializeOwned + Send + Sync + 'static,
    {
        self.get(JWT_AUTH_DATA_KEY)
    }

    #[inline]
    fn jwt_auth_state(&self) -> JwtAuthState {
        self.get(JWT_AUTH_STATE_KEY).cloned().unwrap_or(JwtAuthState::Unauthorized)
    }
}

/// JwtAuth, used as middleware.
pub struct JwtAuth<C> {
    secret: String,
    response_error: bool,
    _claims: PhantomData<C>,
    validation: Validation,
    finders: Vec<Box<dyn JwtTokenFinder>>,
}

impl<C> JwtAuth<C>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    /// Create new `JwtAuth`.
    #[inline]
    pub fn new(secret: String) -> JwtAuth<C> {
        JwtAuth {
            response_error: true,
            secret,
            _claims: PhantomData::<C>,
            finders: vec![Box::new(HeaderFinder::new())],
            validation: Validation::default(),
        }
    }
    /// Get response_error value.
    #[inline]
    pub fn response_error(&self) -> bool {
        self.response_error
    }
    /// Sets response_error value and return Self.
    #[inline]
    pub fn with_response_error(mut self, response_error: bool) -> Self {
        self.response_error = response_error;
        self
    }

    /// Get secret reference.
    #[inline]
    pub fn secret(&self) -> &String {
        &self.secret
    }
    /// Get secret mutable reference.
    #[inline]
    pub fn secret_mut(&mut self) -> &mut String {
        &mut self.secret
    }
    /// Sets secret with new value and return Self.
    #[inline]
    pub fn with_secret(mut self, secret: String) -> Self {
        self.secret = secret;
        self
    }

    /// Get extractor list reference.
    #[inline]
    pub fn finders(&self) -> &Vec<Box<dyn JwtTokenFinder>> {
        &self.finders
    }
    /// Get extractor list mutable reference.
    #[inline]
    pub fn finders_mut(&mut self) -> &mut Vec<Box<dyn JwtTokenFinder>> {
        &mut self.finders
    }
    /// Sets extractor list with new value and return Self.
    #[inline]
    pub fn with_finders(mut self, finders: Vec<Box<dyn JwtTokenFinder>>) -> Self {
        self.finders = finders;
        self
    }

    /// Decode token with secret.
    #[inline]
    pub fn decode(&self, token: &str) -> Result<TokenData<C>, JwtError> {
        decode::<C>(token, &DecodingKey::from_secret(self.secret.as_ref()), &self.validation)
    }

    async fn find_token(&self, req: &mut Request) -> Option<String> {
        for finder in &self.finders {
            if let Some(token) = finder.find_token(req).await {
                return Some(token);
            }
        }
        None
    }
}

#[async_trait]
impl<C> Handler for JwtAuth<C>
where
    C: DeserializeOwned + Send + Sync + 'static,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        let token = self.find_token(req).await;
        if let Some(token) = token {
            if let Ok(data) = self.decode(&token) {
                depot.insert(JWT_AUTH_DATA_KEY, data);
                depot.insert(JWT_AUTH_STATE_KEY, JwtAuthState::Authorized);
            } else {
                depot.insert(JWT_AUTH_STATE_KEY, JwtAuthState::Forbidden);
                if self.response_error {
                    res.set_status_error(StatusError::forbidden());
                    ctrl.skip_rest();
                }
            }
            depot.insert(JWT_AUTH_TOKEN_KEY, token);
            ctrl.call_next(req, depot, res).await;
        } else {
            depot.insert(JWT_AUTH_STATE_KEY, JwtAuthState::Unauthorized);
            if self.response_error {
                res.set_status_error(StatusError::unauthorized());
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use time::{OffsetDateTime, Duration};
    use jsonwebtoken::EncodingKey;
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    struct JwtClaims {
        user: String,
        exp: i64,
    }
    #[tokio::test]
    async fn test_jwt_auth() {
        let auth_handler: JwtAuth<JwtClaims> =
            JwtAuth::new("ABCDEF".into())
                .with_response_error(true)
                .with_finders(vec![
                    Box::new(HeaderFinder::new()),
                    Box::new(QueryFinder::new("jwt_token")),
                    Box::new(CookieFinder::new("jwt_token")),
                ]);

        #[handler]
        async fn hello() -> &'static str {
            "hello"
        }

        let router = Router::new()
            .hoop(auth_handler)
            .push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        async fn access(service: &Service, token: &str) -> String {
            TestClient::get("http://127.0.0.1:5801/hello")
                .add_header("Authorization", format!("Bearer {}", token), true)
                .send(service)
                .await
                .take_string()
                .await
                .unwrap()
        }

        let claim = JwtClaims {
            user: "root".into(),
            exp: (OffsetDateTime::now_utc() + Duration::days(1)).unix_timestamp(),
        };

        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claim,
            &EncodingKey::from_secret(b"ABCDEF"),
        )
        .unwrap();
        let content = access(&service, &token).await;
        assert!(content.contains("hello"));

        let content = TestClient::get(format!("http://127.0.0.1:5801/hello?jwt_token={}", token))
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("hello"));
        let content = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header("Cookie", format!("jwt_token={}", token), true)
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("hello"));

        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claim,
            &EncodingKey::from_secret(b"ABCDEFG"),
        )
        .unwrap();
        let content = access(&service, &token).await;
        assert!(content.contains("Forbidden"));
    }
}
