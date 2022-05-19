//! jwt auth middleware

use std::marker::PhantomData;

pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::{decode, Algorithm, DecodingKey, TokenData, Validation};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;

use salvo_core::async_trait;
use salvo_core::http::header::AUTHORIZATION;
use salvo_core::http::{Method, Request, Response, StatusError};
use salvo_core::routing::FlowCtrl;
use salvo_core::{Depot, Handler};

/// key used to insert auth decoded data to depot.
pub const AUTH_DATA_KEY: &str = "::salvo::extra::jwt_auth::auth_data";
/// key used to insert auth state data to depot.
pub const AUTH_STATE_KEY: &str = "::salvo::extra::jwt_auth::auth_state";
/// key used to insert auth token data to depot.
pub const AUTH_TOKEN_KEY: &str = "::salvo::extra::jwt_auth::auth_token";

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

/// JwtTokenExtractor
#[async_trait]
pub trait JwtTokenExtractor: Send + Sync {
    /// Get token from request.
    async fn token(&self, req: &mut Request) -> Option<String>;
}

/// HeaderExtractor
#[derive(Default)]
pub struct HeaderExtractor {
    cared_methods: Vec<Method>,
}
impl HeaderExtractor {
    /// Create new `HeaderExtractor`.
    #[inline]
    pub fn new() -> Self {
        HeaderExtractor {
            cared_methods: ALL_METHODS.clone(),
        }
    }
    /// Get cated methods list reference.
    #[inline]
    pub fn cared_methods(&self) -> &Vec<Method> {
        &self.cared_methods
    }
    /// Get cated methods list mutable reference.
    #[inline]
    pub fn cared_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.cared_methods
    }
    /// Set cated methods list.
    #[inline]
    pub fn set_cared_methods(&mut self, methods: Vec<Method>) {
        self.cared_methods = methods;
    }
    /// Set cated methods list and returns Self.
    #[inline]
    pub fn with_cared_methods(mut self, methods: Vec<Method>) -> Self {
        self.cared_methods = methods;
        self
    }
}
#[async_trait]
impl JwtTokenExtractor for HeaderExtractor {
    #[inline]
    async fn token(&self, req: &mut Request) -> Option<String> {
        if self.cared_methods.contains(req.method()) {
            if let Some(auth) = req.headers().get(AUTHORIZATION) {
                if let Ok(auth) = auth.to_str() {
                    if auth.starts_with("Bearer") {
                        return auth.split_once(' ').map(|(_, token)| token.to_owned());
                    }
                }
            }
        }
        None
    }
}

/// FormExtractor
pub struct FormExtractor {
    cared_methods: Vec<Method>,
    field_name: String,
}
impl FormExtractor {
    /// Create new `FormExtractor`.
    #[inline]
    pub fn new<T: Into<String>>(field_name: T) -> Self {
        FormExtractor {
            field_name: field_name.into(),
            cared_methods: ALL_METHODS.clone(),
        }
    }
    /// Get cated methods list reference.
    #[inline]
    pub fn cared_methods(&self) -> &Vec<Method> {
        &self.cared_methods
    }
    /// Get cated methods list mutable reference.
    #[inline]
    pub fn cared_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.cared_methods
    }
    /// Set cated methods list.
    #[inline]
    pub fn set_cared_methods(&mut self, methods: Vec<Method>) {
        self.cared_methods = methods;
    }
    /// Set cated methods list and return Self.
    #[inline]
    pub fn with_cared_methods(mut self, methods: Vec<Method>) -> Self {
        self.cared_methods = methods;
        self
    }
}
#[async_trait]
impl JwtTokenExtractor for FormExtractor {
    #[inline]
    async fn token(&self, req: &mut Request) -> Option<String> {
        if self.cared_methods.contains(req.method()) {
            req.form(&self.field_name).await
        } else {
            None
        }
    }
}

/// QueryExtractor
pub struct QueryExtractor {
    cared_methods: Vec<Method>,
    query_name: String,
}
impl QueryExtractor {
    /// Create new `QueryExtractor`.
    #[inline]
    pub fn new<T: Into<String>>(query_name: T) -> Self {
        QueryExtractor {
            query_name: query_name.into(),
            cared_methods: ALL_METHODS.clone(),
        }
    }
    /// Get cated methods list reference.
    #[inline]
    pub fn cared_methods(&self) -> &Vec<Method> {
        &self.cared_methods
    }
    /// Get cated methods list mutable reference.
    #[inline]
    pub fn cared_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.cared_methods
    }
    /// Set cated methods list.
    #[inline]
    pub fn set_cared_methods(&mut self, methods: Vec<Method>) {
        self.cared_methods = methods;
    }
    /// Set cated methods list and return Self.
    #[inline]
    pub fn with_cared_methods(mut self, methods: Vec<Method>) -> Self {
        self.cared_methods = methods;
        self
    }
}

#[async_trait]
impl JwtTokenExtractor for QueryExtractor {
    #[inline]
    async fn token(&self, req: &mut Request) -> Option<String> {
        if self.cared_methods.contains(req.method()) {
            req.query(&self.query_name)
        } else {
            None
        }
    }
}

/// CookieExtractor
pub struct CookieExtractor {
    cared_methods: Vec<Method>,
    cookie_name: String,
}
impl CookieExtractor {
    /// Create new `CookieExtractor`.
    #[inline]
    pub fn new<T: Into<String>>(cookie_name: T) -> Self {
        CookieExtractor {
            cookie_name: cookie_name.into(),
            cared_methods: vec![
                Method::GET,
                Method::HEAD,
                Method::OPTIONS,
                Method::CONNECT,
                Method::TRACE,
            ],
        }
    }
    /// Get cated methods list reference.
    #[inline]
    pub fn cared_methods(&self) -> &Vec<Method> {
        &self.cared_methods
    }
    /// Get cated methods list mutable reference.
    #[inline]
    pub fn cared_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.cared_methods
    }
    /// Set cated methods list.
    #[inline]
    pub fn set_cared_methods(&mut self, methods: Vec<Method>) {
        self.cared_methods = methods;
    }
    /// Set cated methods list and return Self.
    #[inline]
    pub fn with_cared_methods(mut self, methods: Vec<Method>) -> Self {
        self.cared_methods = methods;
        self
    }
}
#[async_trait]
impl JwtTokenExtractor for CookieExtractor {
    #[inline]
    async fn token(&self, req: &mut Request) -> Option<String> {
        if self.cared_methods.contains(req.method()) {
            req.cookie(&self.cookie_name).map(|c| c.value().to_owned())
        } else {
            None
        }
    }
}
/// JwtAuthState
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
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
        C: DeserializeOwned + Sync + Send + 'static;
    /// get jwt auth state from depot.
    fn jwt_auth_state(&self) -> JwtAuthState;
}

impl JwtAuthDepotExt for Depot {
    #[inline]
    fn jwt_auth_token(&self) -> Option<&String> {
        self.get(AUTH_TOKEN_KEY)
    }

    #[inline]
    fn jwt_auth_data<C>(&self) -> Option<&TokenData<C>>
    where
        C: DeserializeOwned + Sync + Send + 'static,
    {
        self.get(AUTH_DATA_KEY)
    }

    #[inline]
    fn jwt_auth_state(&self) -> JwtAuthState {
        self.get(AUTH_STATE_KEY).cloned().unwrap_or(JwtAuthState::Unauthorized)
    }
}

/// JwtAuthHandler, used as middleware.
pub struct JwtAuthHandler<C> {
    secret: String,
    response_error: bool,
    claims: PhantomData<C>,
    validation: Validation,
    extractors: Vec<Box<dyn JwtTokenExtractor>>,
}

impl<C> JwtAuthHandler<C>
where
    C: DeserializeOwned + Sync + Send + 'static,
{
    /// Create new `JwtAuthHandler`.
    #[inline]
    pub fn new(secret: String) -> JwtAuthHandler<C> {
        JwtAuthHandler {
            response_error: true,
            secret,
            claims: PhantomData::<C>,
            extractors: vec![Box::new(HeaderExtractor::new())],
            validation: Validation::default(),
        }
    }
    /// Get response_error value.
    #[inline]
    pub fn response_error(&self) -> bool {
        self.response_error
    }
    /// Set response_error value and return Self.
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
    /// Set secret with new value and return Self.
    #[inline]
    pub fn with_secret(mut self, secret: String) -> Self {
        self.secret = secret;
        self
    }

    /// Get extractor list reference.
    #[inline]
    pub fn extractors(&self) -> &Vec<Box<dyn JwtTokenExtractor>> {
        &self.extractors
    }
    /// Get extractor list mutable reference.
    #[inline]
    pub fn extractors_mut(&mut self) -> &mut Vec<Box<dyn JwtTokenExtractor>> {
        &mut self.extractors
    }
    /// Set extractor list with new value and return Self.
    #[inline]
    pub fn with_extractors(mut self, extractors: Vec<Box<dyn JwtTokenExtractor>>) -> Self {
        self.extractors = extractors;
        self
    }

    /// Decode token with secret.
    #[inline]
    pub fn decode(&self, token: &str) -> Result<TokenData<C>, JwtError> {
        decode::<C>(
            token,
            &DecodingKey::from_secret(&*self.secret.as_ref()),
            &self.validation,
        )
    }
}

#[async_trait]
impl<C> Handler for JwtAuthHandler<C>
where
    C: DeserializeOwned + Sync + Send + 'static,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        for extractor in &self.extractors {
            if let Some(token) = extractor.token(req).await {
                if let Ok(data) = self.decode(&token) {
                    depot.insert(AUTH_DATA_KEY, data);
                    depot.insert(AUTH_STATE_KEY, JwtAuthState::Authorized);
                } else {
                    depot.insert(AUTH_STATE_KEY, JwtAuthState::Forbidden);
                    if self.response_error {
                        res.set_status_error(StatusError::forbidden());
                        ctrl.skip_rest();
                    }
                }
                depot.insert(AUTH_TOKEN_KEY, token);
                ctrl.call_next(req, depot, res).await;
                return;
            }
        }
        depot.insert(AUTH_STATE_KEY, JwtAuthState::Unauthorized);
        if self.response_error {
            res.set_status_error(StatusError::unauthorized());
            ctrl.skip_rest();
        } else {
            ctrl.call_next(req, depot, res).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use jsonwebtoken::EncodingKey;
    use salvo_core::hyper;
    use salvo_core::prelude::*;
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    struct JwtClaims {
        user: String,
        exp: i64,
    }
    #[tokio::test]
    async fn test_jwt_auth() {
        let auth_handler: JwtAuthHandler<JwtClaims> = JwtAuthHandler::new("ABCDEF".into())
            .with_response_error(true)
            .with_extractors(vec![
                Box::new(HeaderExtractor::new()),
                Box::new(QueryExtractor::new("jwt_token")),
                Box::new(CookieExtractor::new("jwt_token")),
            ]);

        #[fn_handler]
        async fn hello() -> &'static str {
            "hello"
        }

        let router = Router::new()
            .hoop(auth_handler)
            .push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        async fn access(service: &Service, token: &str) -> String {
            let req = hyper::Request::builder()
                .method("GET")
                .uri("http://127.0.0.1:7979/hello")
                .header("Authorization", format!("Bearer {}", token))
                .body(hyper::Body::empty())
                .unwrap();
            service.handle(req).await.take_text().await.unwrap()
        }

        let claim = JwtClaims {
            user: "root".into(),
            exp: (Utc::now() + Duration::days(1)).timestamp(),
        };

        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claim,
            &EncodingKey::from_secret(b"ABCDEF"),
        )
        .unwrap();
        let content = access(&service, &token).await;
        assert!(content.contains("hello"));

        let req = hyper::Request::builder()
            .method("GET")
            .uri(format!("http://127.0.0.1:7979/hello?jwt_token={}", token))
            .body(hyper::Body::empty())
            .unwrap();
        let content = service.handle(req).await.take_text().await.unwrap();
        assert!(content.contains("hello"));
        let req = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/hello")
            .header("Cookie", format!("jwt_token={}", token))
            .body(hyper::Body::empty())
            .unwrap();
        let content = service.handle(req).await.take_text().await.unwrap();
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
