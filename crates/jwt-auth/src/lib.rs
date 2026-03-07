//! Provides JWT (JSON Web Token) authentication support for the Salvo web framework.
//!
//! This crate helps you implement JWT-based authentication in your Salvo web applications.
//! It offers flexible token extraction from various sources (headers, query parameters, cookies,
//! etc.) and multiple decoding strategies.
//!
//! # Features
//!
//! - Extract JWT tokens from multiple sources (headers, query parameters, cookies, forms)
//! - Configurable token validation
//! - OpenID Connect support (behind the `oidc` feature flag)
//! - Seamless integration with Salvo's middleware system
//!
//! # Security Considerations
//!
//! **⚠️ WARNING: Avoid passing JWT tokens in URL query parameters in production!**
//!
//! While this crate supports extracting tokens from query parameters (via [`QueryFinder`]),
//! this method has significant security risks:
//!
//! - **Browser history exposure**: URLs with tokens are stored in browser history
//! - **Server logs**: Tokens may be logged in web server access logs
//! - **Referer header leakage**: Tokens can leak to third-party sites via the Referer header
//! - **Shoulder surfing**: Tokens are visible in the address bar
//!
//! **Recommended alternatives:**
//! - Use [`HeaderFinder`] with the `Authorization: Bearer <token>` header (recommended)
//! - Use [`CookieFinder`] with `HttpOnly` and `Secure` cookie flags
//!
//! The example below uses query parameters for simplicity, but **production applications
//! should use the Authorization header or secure cookies instead**.
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
//! #[derive(Serialize, Deserialize, Clone, Debug)]
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
//!     let acceptor = TcpListener::new("0.0.0.0:8698").bind().await;
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

use std::fmt::{self, Debug, Formatter};
use std::marker::PhantomData;

#[doc(no_inline)]
pub use jsonwebtoken::{
    Algorithm, DecodingKey, TokenData, Validation, decode, errors::Error as JwtError,
};
use salvo_core::http::{Method, Request, Response, StatusError};
use salvo_core::{Depot, FlowCtrl, Handler, async_trait};
use serde::de::DeserializeOwned;
use thiserror::Error;

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
    /// Failure of validating the token. See [jsonwebtoken::errors::ErrorKind] for possible reasons
    /// this value could be returned Would typically result in a 401 HTTP Status code
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
    /// The generic parameter `C` should be the same type used when configuring the `JwtAuth`
    /// middleware.
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
impl<C, D> Debug for JwtAuth<C, D>
where
    C: DeserializeOwned + Send + Sync + 'static,
    D: JwtAuthDecoder + Send + Sync + 'static,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("JwtAuth")
            .field("force_passed", &self.force_passed)
            .finish()
    }
}

impl<C, D> JwtAuth<C, D>
where
    C: DeserializeOwned + Send + Sync + 'static,
    D: JwtAuthDecoder + Send + Sync + 'static,
{
    /// Create new `JwtAuth`.
    #[inline]
    #[must_use]
    pub fn new(decoder: D) -> Self {
        Self {
            force_passed: false,
            decoder,
            _claims: PhantomData::<C>,
            finders: vec![Box::new(HeaderFinder::new())],
        }
    }
    /// Sets force_passed value and return Self.
    #[inline]
    #[must_use]
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
    #[must_use]
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
    C: DeserializeOwned + Clone + Send + Sync + 'static,
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

    #[derive(Serialize, Deserialize, Clone, Debug)]
    struct JwtClaims {
        user: String,
        exp: i64,
    }

    fn create_test_token(secret: &[u8], exp_days: i64) -> String {
        let claim = JwtClaims {
            user: "test_user".into(),
            exp: (OffsetDateTime::now_utc() + Duration::days(exp_days)).unix_timestamp(),
        };
        jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claim,
            &EncodingKey::from_secret(secret),
        )
        .unwrap()
    }

    // ==================== ConstDecoder Tests ====================

    #[test]
    fn test_const_decoder_new() {
        let key = DecodingKey::from_secret(b"test_secret");
        let decoder = ConstDecoder::new(key);
        assert!(format!("{:?}", decoder).contains("ConstDecoder"));
    }

    #[test]
    fn test_const_decoder_with_validation() {
        let key = DecodingKey::from_secret(b"test_secret");
        let mut validation = Validation::default();
        validation.validate_exp = false;
        let decoder = ConstDecoder::with_validation(key, validation);
        assert!(format!("{:?}", decoder).contains("ConstDecoder"));
    }

    #[test]
    fn test_const_decoder_from_secret() {
        let decoder = ConstDecoder::from_secret(b"my_secret_key");
        assert!(format!("{:?}", decoder).contains("ConstDecoder"));
    }

    #[test]
    fn test_const_decoder_from_base64_secret() {
        // Valid base64 encoded secret
        let decoder = ConstDecoder::from_base64_secret("dGVzdF9zZWNyZXQ=");
        assert!(decoder.is_ok());

        // Invalid base64 should fail
        let decoder = ConstDecoder::from_base64_secret("not valid base64!!!");
        assert!(decoder.is_err());
    }

    #[tokio::test]
    async fn test_const_decoder_decode_valid_token() {
        let secret = b"test_secret_key";
        let decoder = ConstDecoder::from_secret(secret);
        let token = create_test_token(secret, 1);

        let mut depot = Depot::new();
        let result = decoder.decode::<JwtClaims>(&token, &mut depot).await;
        assert!(result.is_ok());

        let token_data = result.unwrap();
        assert_eq!(token_data.claims.user, "test_user");
    }

    #[tokio::test]
    async fn test_const_decoder_decode_invalid_token() {
        let decoder = ConstDecoder::from_secret(b"correct_secret");
        let token = create_test_token(b"wrong_secret", 1);

        let mut depot = Depot::new();
        let result = decoder.decode::<JwtClaims>(&token, &mut depot).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_const_decoder_decode_expired_token() {
        let secret = b"test_secret_key";
        let decoder = ConstDecoder::from_secret(secret);
        let token = create_test_token(secret, -1); // Expired

        let mut depot = Depot::new();
        let result = decoder.decode::<JwtClaims>(&token, &mut depot).await;
        assert!(result.is_err());
    }

    // ==================== HeaderFinder Tests ====================

    #[test]
    fn test_header_finder_new() {
        let finder = HeaderFinder::new();
        assert_eq!(finder.cared_methods.len(), 9); // All methods
        assert_eq!(finder.header_names.len(), 2); // Authorization + Proxy-Authorization
    }

    #[test]
    fn test_header_finder_cared_methods() {
        let finder = HeaderFinder::new().cared_methods(vec![Method::GET, Method::POST]);
        assert_eq!(finder.cared_methods.len(), 2);
        assert!(finder.cared_methods.contains(&Method::GET));
        assert!(finder.cared_methods.contains(&Method::POST));
    }

    #[test]
    fn test_header_finder_header_names() {
        use salvo_core::http::header::HeaderName;
        let custom_header = HeaderName::from_static("x-custom-auth");
        let finder = HeaderFinder::new().header_names(vec![custom_header.clone()]);
        assert_eq!(finder.header_names.len(), 1);
        assert_eq!(finder.header_names[0], custom_header);
    }

    #[test]
    fn test_header_finder_mut_methods() {
        let mut finder = HeaderFinder::new();
        finder.cared_methods_mut().push(Method::CONNECT);
        finder.header_names_mut().clear();
        assert!(finder.header_names.is_empty());
    }

    // ==================== QueryFinder Tests ====================

    #[test]
    fn test_query_finder_new() {
        let finder = QueryFinder::new("token");
        assert_eq!(finder.query_name, "token");
        assert_eq!(finder.cared_methods.len(), 9);
    }

    #[test]
    fn test_query_finder_with_static_str() {
        let finder = QueryFinder::new("access_token");
        assert_eq!(finder.query_name, "access_token");
    }

    #[test]
    fn test_query_finder_with_string() {
        let finder = QueryFinder::new(String::from("jwt"));
        assert_eq!(finder.query_name, "jwt");
    }

    #[test]
    fn test_query_finder_cared_methods() {
        let finder = QueryFinder::new("token").cared_methods(vec![Method::GET]);
        assert_eq!(finder.cared_methods.len(), 1);
        assert!(finder.cared_methods.contains(&Method::GET));
    }

    // ==================== CookieFinder Tests ====================

    #[test]
    fn test_cookie_finder_new() {
        let finder = CookieFinder::new("session");
        assert_eq!(finder.cookie_name, "session");
        assert_eq!(finder.cared_methods.len(), 9);
    }

    #[test]
    fn test_cookie_finder_cared_methods() {
        let finder = CookieFinder::new("jwt").cared_methods(vec![Method::GET, Method::POST]);
        assert_eq!(finder.cared_methods.len(), 2);
    }

    #[test]
    fn test_cookie_finder_mut_methods() {
        let mut finder = CookieFinder::new("token");
        finder.cared_methods_mut().clear();
        assert!(finder.cared_methods.is_empty());
    }

    // ==================== FormFinder Tests ====================

    #[test]
    fn test_form_finder_new() {
        let finder = FormFinder::new("token");
        assert_eq!(finder.field_name, "token");
        assert_eq!(finder.cared_methods.len(), 9);
    }

    #[test]
    fn test_form_finder_cared_methods() {
        let finder = FormFinder::new("access_token").cared_methods(vec![Method::POST]);
        assert_eq!(finder.cared_methods.len(), 1);
        assert!(finder.cared_methods.contains(&Method::POST));
    }

    // ==================== JwtAuthState Tests ====================

    #[test]
    fn test_jwt_auth_state_eq() {
        assert_eq!(JwtAuthState::Authorized, JwtAuthState::Authorized);
        assert_eq!(JwtAuthState::Unauthorized, JwtAuthState::Unauthorized);
        assert_eq!(JwtAuthState::Forbidden, JwtAuthState::Forbidden);
        assert_ne!(JwtAuthState::Authorized, JwtAuthState::Forbidden);
    }

    #[test]
    fn test_jwt_auth_state_clone() {
        let state = JwtAuthState::Authorized;
        let cloned = state;
        assert_eq!(state, cloned);
    }

    #[test]
    fn test_jwt_auth_state_debug() {
        assert!(format!("{:?}", JwtAuthState::Authorized).contains("Authorized"));
        assert!(format!("{:?}", JwtAuthState::Unauthorized).contains("Unauthorized"));
        assert!(format!("{:?}", JwtAuthState::Forbidden).contains("Forbidden"));
    }

    // ==================== JwtAuthDepotExt Tests ====================

    #[test]
    fn test_depot_jwt_auth_state_default() {
        let depot = Depot::new();
        assert_eq!(depot.jwt_auth_state(), JwtAuthState::Unauthorized);
    }

    #[test]
    fn test_depot_jwt_auth_token_none() {
        let depot = Depot::new();
        assert!(depot.jwt_auth_token().is_none());
    }

    #[test]
    fn test_depot_jwt_auth_data_none() {
        let depot = Depot::new();
        assert!(depot.jwt_auth_data::<JwtClaims>().is_none());
    }

    #[test]
    fn test_depot_jwt_auth_error_none() {
        let depot = Depot::new();
        assert!(depot.jwt_auth_error().is_none());
    }

    #[test]
    fn test_depot_with_jwt_auth_state() {
        let mut depot = Depot::new();
        depot.insert(JWT_AUTH_STATE_KEY, JwtAuthState::Authorized);
        assert_eq!(depot.jwt_auth_state(), JwtAuthState::Authorized);
    }

    #[test]
    fn test_depot_with_jwt_auth_token() {
        let mut depot = Depot::new();
        depot.insert(JWT_AUTH_TOKEN_KEY, String::from("test_token_value"));
        assert_eq!(depot.jwt_auth_token(), Some("test_token_value"));
    }

    // ==================== JwtAuth Tests ====================

    #[test]
    fn test_jwt_auth_new() {
        let decoder = ConstDecoder::from_secret(b"secret");
        let auth: JwtAuth<JwtClaims, _> = JwtAuth::new(decoder);
        assert!(!auth.force_passed);
        assert_eq!(auth.finders.len(), 1); // Default HeaderFinder
    }

    #[test]
    fn test_jwt_auth_force_passed() {
        let decoder = ConstDecoder::from_secret(b"secret");
        let auth: JwtAuth<JwtClaims, _> = JwtAuth::new(decoder).force_passed(true);
        assert!(auth.force_passed);
    }

    #[test]
    fn test_jwt_auth_finders() {
        let decoder = ConstDecoder::from_secret(b"secret");
        let auth: JwtAuth<JwtClaims, _> = JwtAuth::new(decoder).finders(vec![
            Box::new(HeaderFinder::new()),
            Box::new(QueryFinder::new("token")),
        ]);
        assert_eq!(auth.finders.len(), 2);
    }

    #[test]
    fn test_jwt_auth_finders_mut() {
        let decoder = ConstDecoder::from_secret(b"secret");
        let mut auth: JwtAuth<JwtClaims, _> = JwtAuth::new(decoder);
        auth.finders_mut().push(Box::new(CookieFinder::new("jwt")));
        assert_eq!(auth.finders.len(), 2);
    }

    #[test]
    fn test_jwt_auth_decoder_mut() {
        let decoder = ConstDecoder::from_secret(b"secret");
        let mut auth: JwtAuth<JwtClaims, _> = JwtAuth::new(decoder);
        let _ = auth.decoder_mut(); // Just verify it compiles and returns
    }

    #[test]
    fn test_jwt_auth_debug() {
        let decoder = ConstDecoder::from_secret(b"secret");
        let auth: JwtAuth<JwtClaims, _> = JwtAuth::new(decoder);
        let debug_str = format!("{:?}", auth);
        assert!(debug_str.contains("JwtAuth"));
        assert!(debug_str.contains("force_passed"));
    }

    // ==================== Integration Tests ====================

    #[tokio::test]
    async fn test_jwt_auth_header_authorization() {
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
                .add_header("Authorization", format!("Bearer {token}"), true)
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

        let content = TestClient::get(format!("http://127.0.0.1:5801/hello?jwt_token={token}"))
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("hello"));
        let content = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header("Cookie", format!("jwt_token={token}"), true)
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

    #[tokio::test]
    async fn test_jwt_auth_unauthorized_no_token() {
        let auth_handler: JwtAuth<JwtClaims, ConstDecoder> =
            JwtAuth::new(ConstDecoder::from_secret(b"SECRET"));

        #[handler]
        async fn hello() -> &'static str {
            "hello"
        }

        let router = Router::new()
            .hoop(auth_handler)
            .push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        let content = TestClient::get("http://127.0.0.1:5801/hello")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("Unauthorized"));
    }

    #[tokio::test]
    async fn test_jwt_auth_force_passed_no_token() {
        let auth_handler: JwtAuth<JwtClaims, ConstDecoder> =
            JwtAuth::new(ConstDecoder::from_secret(b"SECRET")).force_passed(true);

        #[handler]
        async fn hello(depot: &mut Depot) -> String {
            format!("{:?}", depot.jwt_auth_state())
        }

        let router = Router::new()
            .hoop(auth_handler)
            .push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        let content = TestClient::get("http://127.0.0.1:5801/hello")
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("Unauthorized"));
    }

    #[tokio::test]
    async fn test_jwt_auth_force_passed_invalid_token() {
        let auth_handler: JwtAuth<JwtClaims, ConstDecoder> =
            JwtAuth::new(ConstDecoder::from_secret(b"SECRET")).force_passed(true);

        #[handler]
        async fn hello(depot: &mut Depot) -> String {
            format!("{:?}", depot.jwt_auth_state())
        }

        let router = Router::new()
            .hoop(auth_handler)
            .push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        let content = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header("Authorization", "Bearer invalid_token", true)
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("Forbidden"));
    }

    #[tokio::test]
    async fn test_jwt_auth_depot_data_access() {
        let auth_handler: JwtAuth<JwtClaims, ConstDecoder> =
            JwtAuth::new(ConstDecoder::from_secret(b"SECRET"));

        #[handler]
        async fn hello(depot: &mut Depot) -> String {
            match depot.jwt_auth_data::<JwtClaims>() {
                Some(data) => format!("user:{}", data.claims.user),
                None => "no_data".to_string(),
            }
        }

        let router = Router::new()
            .hoop(auth_handler)
            .push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        let claim = JwtClaims {
            user: "admin".into(),
            exp: (OffsetDateTime::now_utc() + Duration::days(1)).unix_timestamp(),
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claim,
            &EncodingKey::from_secret(b"SECRET"),
        )
        .unwrap();

        let content = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header("Authorization", format!("Bearer {token}"), true)
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("user:admin"));
    }

    #[tokio::test]
    async fn test_jwt_auth_proxy_authorization_header() {
        let auth_handler: JwtAuth<JwtClaims, ConstDecoder> =
            JwtAuth::new(ConstDecoder::from_secret(b"SECRET"));

        #[handler]
        async fn hello() -> &'static str {
            "hello"
        }

        let router = Router::new()
            .hoop(auth_handler)
            .push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        let claim = JwtClaims {
            user: "proxy_user".into(),
            exp: (OffsetDateTime::now_utc() + Duration::days(1)).unix_timestamp(),
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claim,
            &EncodingKey::from_secret(b"SECRET"),
        )
        .unwrap();

        let content = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header("Proxy-Authorization", format!("Bearer {token}"), true)
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("hello"));
    }

    #[tokio::test]
    async fn test_header_finder_non_bearer_ignored() {
        let auth_handler: JwtAuth<JwtClaims, ConstDecoder> =
            JwtAuth::new(ConstDecoder::from_secret(b"SECRET"));

        #[handler]
        async fn hello() -> &'static str {
            "hello"
        }

        let router = Router::new()
            .hoop(auth_handler)
            .push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        // Basic auth should be ignored
        let content = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header("Authorization", "Basic dXNlcjpwYXNz", true)
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("Unauthorized"));
    }

    #[tokio::test]
    async fn test_header_finder_method_filtering() {
        let auth_handler: JwtAuth<JwtClaims, ConstDecoder> =
            JwtAuth::new(ConstDecoder::from_secret(b"SECRET")).finders(vec![Box::new(
                HeaderFinder::new().cared_methods(vec![Method::POST]),
            )]);

        #[handler]
        async fn hello() -> &'static str {
            "hello"
        }

        let router = Router::new()
            .hoop(auth_handler)
            .push(Router::with_path("hello").get(hello).post(hello));
        let service = Service::new(router);

        let claim = JwtClaims {
            user: "test".into(),
            exp: (OffsetDateTime::now_utc() + Duration::days(1)).unix_timestamp(),
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claim,
            &EncodingKey::from_secret(b"SECRET"),
        )
        .unwrap();

        // GET should not find token (method not cared)
        let content = TestClient::get("http://127.0.0.1:5801/hello")
            .add_header("Authorization", format!("Bearer {token}"), true)
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("Unauthorized"));

        // POST should find token
        let content = TestClient::post("http://127.0.0.1:5801/hello")
            .add_header("Authorization", format!("Bearer {token}"), true)
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("hello"));
    }
}
