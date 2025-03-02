//! Provides JWT (JSON Web Token) authentication support for the Salvo web framework.
//!
//! This crate helps you implement JWT-based authentication in your Salvo web applications.
//! It offers flexible token extraction from various sources (headers, query parameters, cookies, etc.)
//! and multiple decoding strategies.
//!
//! # Features
//!
//! - Extract JWT tokens from multiple sources (headers, query parameters, cookies, forms)
//! - Configurable token validation
//! - OpenID Connect support (behind the `oidc` feature flag)
//! - Seamless integration with Salvo's middleware system
//!
//! # Example:
//!
//! ```no_run
//! use jsonwebtoken::{self, EncodingKey};
//! use salvo::http::{Method, StatusError};
//! use salvo::jwt_auth::{ConstDecoder, QueryFinder};
//! use salvo::prelude::*;
//! use serde::{Deserialize, Serialize};
//! use time::{Duration, OffsetDateTime};
//!
//! const SECRET_KEY: &str = "YOUR_SECRET_KEY"; // In production, use a secure key management solution
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! pub struct JwtClaims {
//!     username: String,
//!     exp: i64,
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let auth_handler: JwtAuth<JwtClaims, _> = JwtAuth::new(ConstDecoder::from_secret(SECRET_KEY.as_bytes()))
//!         .finders(vec![
//!             // Box::new(HeaderFinder::new()),
//!             Box::new(QueryFinder::new("jwt_token")),
//!             // Box::new(CookieFinder::new("jwt_token")),
//!         ])
//!         .force_passed(true);
//!
//!     let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
//!     Server::new(acceptor)
//!         .serve(Router::with_hoop(auth_handler).goal(index))
//!         .await;
//! }
//! #[handler]
//! async fn index(req: &mut Request, depot: &mut Depot, res: &mut Response) -> anyhow::Result<()> {
//!     if req.method() == Method::POST {
//!         let (username, password) = (
//!             req.form::<String>("username").await.unwrap_or_default(),
//!             req.form::<String>("password").await.unwrap_or_default(),
//!         );
//!         if !validate(&username, &password) {
//!             res.render(Text::Html(LOGIN_HTML));
//!             return Ok(());
//!         }
//!         let exp = OffsetDateTime::now_utc() + Duration::days(14);
//!         let claim = JwtClaims {
//!             username,
//!             exp: exp.unix_timestamp(),
//!         };
//!         let token = jsonwebtoken::encode(
//!             &jsonwebtoken::Header::default(),
//!             &claim,
//!             &EncodingKey::from_secret(SECRET_KEY.as_bytes()),
//!         )?;
//!         res.render(Redirect::other(format!("/?jwt_token={token}")));
//!     } else {
//!         match depot.jwt_auth_state() {
//!             JwtAuthState::Authorized => {
//!                 let data = depot.jwt_auth_data::<JwtClaims>().unwrap();
//!                 res.render(Text::Plain(format!(
//!                     "Hi {}, you have logged in successfully!",
//!                     data.claims.username
//!                 )));
//!             }
//!             JwtAuthState::Unauthorized => {
//!                 res.render(Text::Html(LOGIN_HTML));
//!             }
//!             JwtAuthState::Forbidden => {
//!                 res.render(StatusError::forbidden());
//!             }
//!         }
//!     }
//!     Ok(())
//! }
//!
//! fn validate(username: &str, password: &str) -> bool {
//!     // In a real application, use secure password verification
//!     username == "root" && password == "pwd"
//! }
//!
//! static LOGIN_HTML: &str = r#"<!DOCTYPE html>
//! <html>
//!     <head>
//!         <title>JWT Auth Demo</title>
//!     </head>
//!     <body>
//!         <h1>JWT Auth</h1>
//!         <form action="/" method="post">
//!         <label for="username"><b>Username</b></label>
//!         <input type="text" placeholder="Enter Username" name="username" required>
//!
//!         <label for="password"><b>Password</b></label>
//!         <input type="password" placeholder="Enter Password" name="password" required>
//!
//!         <button type="submit">Login</button>
//!     </form>
//!     </body>
//! </html>
//! "#;
//! ```

#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::marker::PhantomData;

#[doc(no_inline)]
pub use jsonwebtoken::{
    Algorithm, DecodingKey, TokenData, Validation, decode, errors::Error as JwtError,
};
use serde::de::DeserializeOwned;
use thiserror::Error;

use salvo_core::http::{Method, Request, Response, StatusError};
use salvo_core::{Depot, FlowCtrl, Handler, async_trait};

mod finder;
pub use finder::{CookieFinder, FormFinder, HeaderFinder, JwtTokenFinder, QueryFinder};

mod decoder;
pub use decoder::{ConstDecoder, JwtAuthDecoder};

#[macro_use]
mod cfg;

cfg_feature! {
    #![feature = "oidc"]
    pub mod oidc;
    pub use oidc::OidcDecoder;
}

/// key used to insert auth decoded data to depot.
pub const JWT_AUTH_DATA_KEY: &str = "::salvo::jwt_auth::auth_data";
/// key used to insert auth state data to depot.
pub const JWT_AUTH_STATE_KEY: &str = "::salvo::jwt_auth::auth_state";
/// key used to insert auth token data to depot.
pub const JWT_AUTH_TOKEN_KEY: &str = "::salvo::jwt_auth::auth_token";
/// key used to insert auth error to depot.
pub const JWT_AUTH_ERROR_KEY: &str = "::salvo::jwt_auth::auth_error";

const ALL_METHODS: [Method; 9] = [
    Method::GET,
    Method::POST,
    Method::PUT,
    Method::DELETE,
    Method::HEAD,
    Method::OPTIONS,
    Method::CONNECT,
    Method::PATCH,
    Method::TRACE,
];

/// JwtAuthError
#[derive(Debug, Error)]
pub enum JwtAuthError {
    /// HTTP client error
    #[cfg(feature = "oidc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "oidc")))]
    #[error("ClientError")]
    ClientError(#[from] hyper_util::client::legacy::Error),

    /// Error occurred in hyper.
    #[cfg(feature = "oidc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "oidc")))]
    #[error("HyperError")]
    Hyper(#[from] salvo_core::hyper::Error),

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
    /// JWT is missing kid, alg, or decoding components
    #[error("JWT is missing kid, alg, or decoding components")]
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

/// Possible states of JWT authentication.
///
/// The middleware sets this state in the depot after processing a request.
/// You can access it via `depot.jwt_auth_state()`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum JwtAuthState {
    /// Authentication was successful and the token was valid.
    Authorized,
    /// No token was provided in the request.
    /// Usually results in a 401 Unauthorized response unless `force_passed` is true.
    Unauthorized,
    /// A token was provided but it failed validation.
    /// Usually results in a 403 Forbidden response unless `force_passed` is true.
    Forbidden,
}

/// Extension trait for accessing JWT authentication data from the depot.
///
/// This trait provides convenient methods to retrieve JWT authentication information
/// that was previously stored in the depot by the `JwtAuth` middleware.
pub trait JwtAuthDepotExt {
    /// Gets the JWT token string from the depot.
    fn jwt_auth_token(&self) -> Option<&str>;

    /// Gets the decoded JWT claims data from the depot.
    ///
    /// The generic parameter `C` should be the same type used when configuring the `JwtAuth` middleware.
    fn jwt_auth_data<C>(&self) -> Option<&TokenData<C>>
    where
        C: DeserializeOwned + Send + Sync + 'static;

    /// Gets the current JWT authentication state from the depot.
    ///
    /// Returns `JwtAuthState::Unauthorized` if no state is present in the depot.
    fn jwt_auth_state(&self) -> JwtAuthState;

    /// Gets the JWT error if authentication failed.
    fn jwt_auth_error(&self) -> Option<&JwtError>;
}

impl JwtAuthDepotExt for Depot {
    #[inline]
    fn jwt_auth_token(&self) -> Option<&str> {
        self.get::<String>(JWT_AUTH_TOKEN_KEY).map(|v| &**v).ok()
    }

    #[inline]
    fn jwt_auth_data<C>(&self) -> Option<&TokenData<C>>
    where
        C: DeserializeOwned + Send + Sync + 'static,
    {
        self.get(JWT_AUTH_DATA_KEY).ok()
    }

    #[inline]
    fn jwt_auth_state(&self) -> JwtAuthState {
        self.get(JWT_AUTH_STATE_KEY)
            .ok()
            .cloned()
            .unwrap_or(JwtAuthState::Unauthorized)
    }

    #[inline]
    fn jwt_auth_error(&self) -> Option<&JwtError> {
        self.get(JWT_AUTH_ERROR_KEY).ok()
    }
}

/// JWT Authentication middleware for Salvo.
///
/// `JwtAuth` extracts and validates JWT tokens from incoming requests based on the configured
/// token finders and decoder. If valid, it stores the decoded data in the depot for later use.
///
/// # Type Parameters
///
/// * `C` - The claims type that will be deserialized from the JWT payload.
/// * `D` - The decoder implementation used to validate and decode the JWT token.
#[non_exhaustive]
pub struct JwtAuth<C, D> {
    /// When set to `true`, the middleware will allow the request to proceed even if
    /// authentication fails, storing only the authentication state in the depot.
    ///
    /// When set to `false` (default), requests with invalid or missing tokens will be
    /// immediately rejected with appropriate status codes.
    pub force_passed: bool,
    _claims: PhantomData<C>,
    /// The decoder used to validate and decode the JWT token.
    pub decoder: D,
    /// A list of token finders that will be used to extract the token from the request.
    /// Finders are tried in order until one returns a token.
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
            force_passed: false,
            decoder,
            _claims: PhantomData::<C>,
            finders: vec![Box::new(HeaderFinder::new())],
        }
    }
    /// Sets force_passed value and return Self.
    #[inline]
    pub fn force_passed(mut self, force_passed: bool) -> Self {
        self.force_passed = force_passed;
        self
    }

    /// Get decoder mutable reference.
    #[inline]
    pub fn decoder_mut(&mut self) -> &mut D {
        &mut self.decoder
    }

    /// Gets a mutable reference to the extractor list.
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
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let token = self.find_token(req).await;
        if let Some(token) = token {
            match self.decoder.decode::<C>(&token, depot).await {
                Ok(data) => {
                    depot.insert(JWT_AUTH_DATA_KEY, data);
                    depot.insert(JWT_AUTH_STATE_KEY, JwtAuthState::Authorized);
                    depot.insert(JWT_AUTH_TOKEN_KEY, token);
                }
                Err(e) => {
                    tracing::info!(error = ?e, "jwt auth error");
                    depot.insert(JWT_AUTH_STATE_KEY, JwtAuthState::Forbidden);
                    depot.insert(JWT_AUTH_ERROR_KEY, e);
                    if !self.force_passed {
                        res.render(StatusError::forbidden());
                        ctrl.skip_rest();
                    }
                }
            }
        } else {
            depot.insert(JWT_AUTH_STATE_KEY, JwtAuthState::Unauthorized);
            if !self.force_passed {
                res.render(StatusError::unauthorized());
                ctrl.skip_rest();
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
        let auth_handler: JwtAuth<JwtClaims, ConstDecoder> =
            JwtAuth::new(ConstDecoder::from_secret(b"ABCDEF")).finders(vec![
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
