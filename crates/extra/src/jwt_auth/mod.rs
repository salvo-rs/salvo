//! jwt auth middleware

use std::marker::PhantomData;

pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::{decode, Algorithm, DecodingKey, TokenData, Validation};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use thiserror::Error;

use salvo_core::http::{Method, Request, Response, StatusError};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};

mod finder;
pub use finder::{CookieFinder, FormFinder, HeaderFinder, JwtTokenFinder, QueryFinder};

mod decoder;
pub use decoder::{ConstDecoder, JwtAuthDecoder};

pub mod oidc;
pub use oidc::OidcDecoder;

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

/// JwtAuthError
#[derive(Debug, Error)]
pub enum JwtAuthError {
    /// HTTP request failed
    #[error("HTTP request failed")]
    ReqwestError(#[from] reqwest::Error),
    /// InvalidUri
    #[error("InvalidUri")]
    InvalidUri(#[from] salvo_core::http::uri::InvalidUri),
    /// Serde error
    #[error("Serde error")]
    SerdeError(#[from] serde_json::Error),
    /// Failed to discover OIDC configuration
    #[error("Failed to discover OIDC configuration")]
    DiscoverError,
    /// Decoding of JWKS error
    #[error("Decoding of JWKS error")]
    DecodeError(#[from] base64::DecodeError),
    /// JWT was missing kid, alg, or decoding components
    #[error("JWT was missing kid, alg, or decoding components")]
    InvalidJwk,
    /// Issuer URL invalid
    #[error("Issuer URL invalid")]
    IssuerParseError,
    /// Failure of validating the token. See [jsonwebtoken::errors::ErrorKind] for possible reasons this value could be returned
    /// Would typically result in a 401 HTTP Status code
    #[error("JWT Is Invalid")]
    ValidationFailed(#[from] jsonwebtoken::errors::Error),
    /// Failure to re-validate the JWKS.
    /// Would typically result in a 401 or 500 status code depending on preference
    #[error("Token was unable to be validated due to cache expiration")]
    CacheError,
    /// Token did not contain a kid in its header and would be impossible to validate
    /// Would typically result in a 401 HTTP Status code
    #[error("Token did not contain a KID field")]
    MissingKid,
}

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
        self.get(JWT_AUTH_STATE_KEY)
            .cloned()
            .unwrap_or(JwtAuthState::Unauthorized)
    }
}

/// JwtAuth, used as middleware.
#[non_exhaustive]
pub struct JwtAuth<C, D> {
    /// Response error directly when set to true.
    pub response_error: bool,
    _claims: PhantomData<C>,
    /// The decoder.
    pub decoder: D,
    /// The finders list.
    pub finders: Vec<Box<dyn JwtTokenFinder>>,
}

impl<C, D> JwtAuth<C, D>
where
    C: DeserializeOwned + Send + Sync + 'static,
    D: JwtAuthDecoder + Send + Sync + 'static,
{
    /// Create new `JwtAuth`.
    #[inline]
    pub fn new(decoder: D) -> Self {
        JwtAuth {
            response_error: true,
            decoder,
            _claims: PhantomData::<C>,
            finders: vec![Box::new(HeaderFinder::new())],
        }
    }
    /// Sets response_error value and return Self.
    #[inline]
    pub fn response_error(mut self, response_error: bool) -> Self {
        self.response_error = response_error;
        self
    }

    /// Get decoder mutable reference.
    #[inline]
    pub fn decoder_mut(&mut self) -> &mut D {
        &mut self.decoder
    }

    /// Get extractor list mutable reference.
    #[inline]
    pub fn finders_mut(&mut self) -> &mut Vec<Box<dyn JwtTokenFinder>> {
        &mut self.finders
    }
    /// Sets extractor list with new value and return Self.
    #[inline]
    pub fn finders(mut self, finders: Vec<Box<dyn JwtTokenFinder>>) -> Self {
        self.finders = finders;
        self
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
impl<C, D> Handler for JwtAuth<C, D>
where
    C: DeserializeOwned + Send + Sync + 'static,
    D: JwtAuthDecoder + Send + Sync + 'static,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        let token = self.find_token(req).await;
        if let Some(token) = token {
            if let Ok(data) = self.decoder.decode::<C>(&token, depot).await {
                depot.insert(JWT_AUTH_DATA_KEY, data);
                depot.insert(JWT_AUTH_STATE_KEY, JwtAuthState::Authorized);
            } else {
                depot.insert(JWT_AUTH_STATE_KEY, JwtAuthState::Forbidden);
                if self.response_error {
                    res.render(StatusError::forbidden());
                    ctrl.skip_rest();
                }
            }
            depot.insert(JWT_AUTH_TOKEN_KEY, token);
            ctrl.call_next(req, depot, res).await;
        } else {
            depot.insert(JWT_AUTH_STATE_KEY, JwtAuthState::Unauthorized);
            if self.response_error {
                res.render(StatusError::unauthorized());
                ctrl.skip_rest();
            } else {
                ctrl.call_next(req, depot, res).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::EncodingKey;
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};
    use serde::{Deserialize, Serialize};
    use time::{Duration, OffsetDateTime};

    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    struct JwtClaims {
        user: String,
        exp: i64,
    }
    #[tokio::test]
    async fn test_jwt_auth() {
        let auth_handler: JwtAuth<JwtClaims, ConstDecoder> = JwtAuth::new(ConstDecoder::new("ABCDEF")).response_error(true).finders(vec![
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