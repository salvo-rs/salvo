//! jwt auth middleware

use async_trait::async_trait;
pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::{decode, Algorithm, DecodingKey, TokenData, Validation};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use std::marker::PhantomData;

use salvo_core::http::errors::*;
use salvo_core::http::header::AUTHORIZATION;
use salvo_core::http::Method;
use salvo_core::http::{Request, Response};
use salvo_core::Depot;
use salvo_core::Handler;

pub const AUTH_CLAIMS_KEY: &str = "::salvo::extra::jwt_auth::auth_data";
pub const AUTH_STATE_KEY: &str = "::salvo::extra::jwt_auth::auth_state";
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

#[async_trait]
pub trait JwtExtractor: Send + Sync {
    async fn get_token(&self, req: &mut Request) -> Option<String>;
}

#[derive(Default)]
pub struct HeaderExtractor {
    cared_methods: Vec<Method>,
}
impl HeaderExtractor {
    pub fn new() -> Self {
        HeaderExtractor {
            cared_methods: ALL_METHODS.clone(),
        }
    }
    pub fn cared_methods(&self) -> &Vec<Method> {
        &self.cared_methods
    }
    pub fn cared_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.cared_methods
    }
    pub fn set_cared_methods(&mut self, methods: Vec<Method>) {
        self.cared_methods = methods;
    }
    pub fn with_cared_methods(mut self, methods: Vec<Method>) -> Self {
        self.cared_methods = methods;
        self
    }
}
#[async_trait]
impl JwtExtractor for HeaderExtractor {
    async fn get_token(&self, req: &mut Request) -> Option<String> {
        if self.cared_methods.contains(req.method()) {
            if let Some(auth) = req.headers().get(AUTHORIZATION) {
                if let Ok(auth) = auth.to_str() {
                    if auth.starts_with("Bearer") {
                        return auth.splitn(2, ' ').collect::<Vec<&str>>().pop().map(|s| s.to_owned());
                    }
                }
            }
        }
        None
    }
}

pub struct FormExtractor {
    cared_methods: Vec<Method>,
    field_name: String,
}
impl FormExtractor {
    pub fn new<T: Into<String>>(field_name: T) -> Self {
        FormExtractor {
            field_name: field_name.into(),
            cared_methods: ALL_METHODS.clone(),
        }
    }
    pub fn cared_methods(&self) -> &Vec<Method> {
        &self.cared_methods
    }
    pub fn cared_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.cared_methods
    }
    pub fn set_cared_methods(&mut self, methods: Vec<Method>) {
        self.cared_methods = methods;
    }
    pub fn with_cared_methods(mut self, methods: Vec<Method>) -> Self {
        self.cared_methods = methods;
        self
    }
}
#[async_trait]
impl JwtExtractor for FormExtractor {
    async fn get_token(&self, req: &mut Request) -> Option<String> {
        if self.cared_methods.contains(req.method()) {
            req.get_form(&self.field_name).await
        } else {
            None
        }
    }
}

pub struct QueryExtractor {
    cared_methods: Vec<Method>,
    query_name: String,
}
impl QueryExtractor {
    pub fn new<T: Into<String>>(query_name: T) -> Self {
        QueryExtractor {
            query_name: query_name.into(),
            cared_methods: ALL_METHODS.clone(),
        }
    }
    pub fn cared_methods(&self) -> &Vec<Method> {
        &self.cared_methods
    }
    pub fn cared_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.cared_methods
    }
    pub fn set_cared_methods(&mut self, methods: Vec<Method>) {
        self.cared_methods = methods;
    }
    pub fn with_cared_methods(mut self, methods: Vec<Method>) -> Self {
        self.cared_methods = methods;
        self
    }
}

#[async_trait]
impl JwtExtractor for QueryExtractor {
    async fn get_token(&self, req: &mut Request) -> Option<String> {
        if self.cared_methods.contains(req.method()) {
            req.get_query(&self.query_name)
        } else {
            None
        }
    }
}

pub struct CookieExtractor {
    cared_methods: Vec<Method>,
    cookie_name: String,
}
impl CookieExtractor {
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
    pub fn cared_methods(&self) -> &Vec<Method> {
        &self.cared_methods
    }
    pub fn cared_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.cared_methods
    }
    pub fn set_cared_methods(&mut self, methods: Vec<Method>) {
        self.cared_methods = methods;
    }
    pub fn with_cared_methods(mut self, methods: Vec<Method>) -> Self {
        self.cared_methods = methods;
        self
    }
}
#[async_trait]
impl JwtExtractor for CookieExtractor {
    async fn get_token(&self, req: &mut Request) -> Option<String> {
        if self.cared_methods.contains(req.method()) {
            req.get_cookie(&self.cookie_name).map(|c| c.value().to_owned())
        } else {
            None
        }
    }
}
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum JwtAuthState {
    Authorized,
    Unauthorized,
    Forbidden,
}
pub trait JwtAuthDepotExt {
    fn jwt_auth_token(&self) -> Option<&String>;
    fn jwt_auth_claims<C>(&self) -> Option<&C>
    where
        C: DeserializeOwned + Sync + Send + 'static;
    fn jwt_auth_state(&self) -> JwtAuthState;
}

impl JwtAuthDepotExt for Depot {
    fn jwt_auth_token(&self) -> Option<&String> {
        self.try_borrow(AUTH_TOKEN_KEY)
    }

    fn jwt_auth_claims<C>(&self) -> Option<&C>
    where
        C: DeserializeOwned + Sync + Send + 'static,
    {
        self.try_borrow(AUTH_CLAIMS_KEY)
    }

    fn jwt_auth_state(&self) -> JwtAuthState {
        self.try_borrow(AUTH_STATE_KEY)
            .cloned()
            .unwrap_or(JwtAuthState::Unauthorized)
    }
}

pub struct JwtAuthHandler<C> {
    secret: String,
    response_error: bool,
    claims: PhantomData<C>,
    validation: Validation,
    extractors: Vec<Box<dyn JwtExtractor>>,
}

impl<C> JwtAuthHandler<C>
where
    C: DeserializeOwned + Sync + Send + 'static,
{
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
    #[inline]
    pub fn response_error(&self) -> bool {
        self.response_error
    }
    #[inline]
    pub fn with_response_error(mut self, response_error: bool) -> Self {
        self.response_error = response_error;
        self
    }

    #[inline]
    pub fn secret(&self) -> &String {
        &self.secret
    }
    #[inline]
    pub fn secret_mut(&mut self) -> &mut String {
        &mut self.secret
    }
    #[inline]
    pub fn with_secret(mut self, secret: String) -> Self {
        self.secret = secret;
        self
    }

    #[inline]
    pub fn extractors(&self) -> &Vec<Box<dyn JwtExtractor>> {
        &self.extractors
    }
    #[inline]
    pub fn extractors_mut(&mut self) -> &mut Vec<Box<dyn JwtExtractor>> {
        &mut self.extractors
    }
    #[inline]
    pub fn with_extractors(mut self, extractors: Vec<Box<dyn JwtExtractor>>) -> Self {
        self.extractors = extractors;
        self
    }

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
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        for extractor in &self.extractors {
            if let Some(token) = extractor.get_token(req).await {
                if let Ok(data) = self.decode(&token) {
                    depot.insert(AUTH_CLAIMS_KEY, data);
                    depot.insert(AUTH_STATE_KEY, JwtAuthState::Authorized);
                } else {
                    depot.insert(AUTH_STATE_KEY, JwtAuthState::Forbidden);
                    if self.response_error {
                        res.set_http_error(Forbidden());
                    }
                }
                depot.insert(AUTH_TOKEN_KEY, token);
                return;
            }
        }
        depot.insert(AUTH_STATE_KEY, JwtAuthState::Unauthorized);
        if self.response_error {
            res.set_http_error(Unauthorized());
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use jsonwebtoken::EncodingKey;
    use salvo_core::hyper;
    use salvo_core::prelude::*;

    use super::*;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct JwtClaims {
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
            .before(auth_handler)
            .push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        async fn access(service: &Service, token: &str) -> String {
            let req: Request = hyper::Request::builder()
                .method("GET")
                .uri("http://127.0.0.1:7979/hello")
                .header("Authorization", format!("Bearer {}", token))
                .body(hyper::Body::empty())
                .unwrap()
                .into();
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

        let req: Request = hyper::Request::builder()
            .method("GET")
            .uri(format!("http://127.0.0.1:7979/hello?jwt_token={}", token))
            .body(hyper::Body::empty())
            .unwrap()
            .into();
        let content = service.handle(req).await.take_text().await.unwrap();
        assert!(content.contains("hello"));
        let req: Request = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/hello")
            .header("Cookie", format!("jwt_token={}", token))
            .body(hyper::Body::empty())
            .unwrap()
            .into();
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
