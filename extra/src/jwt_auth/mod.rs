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
    catch_methods: Vec<Method>,
}
impl HeaderExtractor {
    pub fn new() -> Self {
        HeaderExtractor {
            catch_methods: ALL_METHODS.clone(),
        }
    }
    pub fn catch_methods(&self) -> &Vec<Method> {
        &self.catch_methods
    }
    pub fn catch_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.catch_methods
    }
    pub fn set_catch_methods(&mut self, methods: Vec<Method>) {
        self.catch_methods = methods;
    }
    pub fn with_catch_methods(mut self, methods: Vec<Method>) -> Self {
        self.catch_methods = methods;
        self
    }
}
#[async_trait]
impl JwtExtractor for HeaderExtractor {
    async fn get_token(&self, req: &mut Request) -> Option<String> {
        if self.catch_methods.contains(req.method()) {
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
    catch_methods: Vec<Method>,
    field_name: String,
}
impl FormExtractor {
    pub fn new<T: Into<String>>(field_name: T) -> Self {
        FormExtractor {
            field_name: field_name.into(),
            catch_methods: ALL_METHODS.clone(),
        }
    }
    pub fn catch_methods(&self) -> &Vec<Method> {
        &self.catch_methods
    }
    pub fn catch_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.catch_methods
    }
    pub fn set_catch_methods(&mut self, methods: Vec<Method>) {
        self.catch_methods = methods;
    }
    pub fn with_catch_methods(mut self, methods: Vec<Method>) -> Self {
        self.catch_methods = methods;
        self
    }
}
#[async_trait]
impl JwtExtractor for FormExtractor {
    async fn get_token(&self, req: &mut Request) -> Option<String> {
        if self.catch_methods.contains(req.method()) {
            req.get_form(&self.field_name).await
        } else {
            None
        }
    }
}

pub struct QueryExtractor {
    catch_methods: Vec<Method>,
    query_name: String,
}
impl QueryExtractor {
    pub fn new<T: Into<String>>(query_name: T) -> Self {
        QueryExtractor {
            query_name: query_name.into(),
            catch_methods: ALL_METHODS.clone(),
        }
    }
    pub fn catch_methods(&self) -> &Vec<Method> {
        &self.catch_methods
    }
    pub fn catch_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.catch_methods
    }
    pub fn set_catch_methods(&mut self, methods: Vec<Method>) {
        self.catch_methods = methods;
    }
    pub fn with_catch_methods(mut self, methods: Vec<Method>) -> Self {
        self.catch_methods = methods;
        self
    }
}

#[async_trait]
impl JwtExtractor for QueryExtractor {
    async fn get_token(&self, req: &mut Request) -> Option<String> {
        if self.catch_methods.contains(req.method()) {
            req.get_query(&self.query_name)
        } else {
            None
        }
    }
}

pub struct CookieExtractor {
    catch_methods: Vec<Method>,
    cookie_name: String,
}
impl CookieExtractor {
    pub fn new<T: Into<String>>(cookie_name: T) -> Self {
        CookieExtractor {
            cookie_name: cookie_name.into(),
            catch_methods: vec![Method::GET],
        }
    }
    pub fn catch_methods(&self) -> &Vec<Method> {
        &self.catch_methods
    }
    pub fn catch_methods_mut(&mut self) -> &mut Vec<Method> {
        &mut self.catch_methods
    }
    pub fn set_catch_methods(&mut self, methods: Vec<Method>) {
        self.catch_methods = methods;
    }
    pub fn with_catch_methods(mut self, methods: Vec<Method>) -> Self {
        self.catch_methods = methods;
        self
    }
}
#[async_trait]
impl JwtExtractor for CookieExtractor {
    async fn get_token(&self, req: &mut Request) -> Option<String> {
        if self.catch_methods.contains(req.method()) {
            req.get_cookie(&self.cookie_name).map(|c| c.value().to_owned())
        } else {
            None
        }
    }
}

pub struct JwtHandler<C> {
    secret: String,
    context_token_key: Option<String>,
    context_data_key: Option<String>,
    context_state_key: Option<String>,
    response_error: bool,
    claims: PhantomData<C>,
    validation: Validation,
    extractors: Vec<Box<dyn JwtExtractor>>,
}

impl<C> JwtHandler<C>
where
    C: DeserializeOwned + Sync + Send + 'static,
{
    #[inline]
    pub fn new(secret: String) -> JwtHandler<C> {
        JwtHandler {
            response_error: true,
            context_token_key: Some("jwt_token".to_owned()),
            context_data_key: Some("jwt_data".to_owned()),
            context_state_key: Some("jwt_state".to_owned()),
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
    pub fn context_token_key(&self) -> Option<&String> {
        self.context_token_key.as_ref()
    }
    #[inline]
    pub fn with_context_token_key(mut self, context_token_key: Option<String>) -> Self {
        self.context_token_key = context_token_key;
        self
    }

    #[inline]
    pub fn context_data_key(&self) -> Option<&String> {
        self.context_data_key.as_ref()
    }
    #[inline]
    pub fn with_context_data_key(mut self, context_data_key: Option<String>) -> Self {
        self.context_data_key = context_data_key;
        self
    }

    #[inline]
    pub fn context_state_key(&self) -> Option<&String> {
        self.context_state_key.as_ref()
    }
    #[inline]
    pub fn with_context_state_key(mut self, context_state_key: Option<String>) -> Self {
        self.context_state_key = context_state_key;
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
impl<C> Handler for JwtHandler<C>
where
    C: DeserializeOwned + Sync + Send + 'static,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        for extractor in &self.extractors {
            if let Some(token) = extractor.get_token(req).await {
                if let Ok(data) = self.decode(&token) {
                    if let Some(key) = &self.context_data_key {
                        depot.insert(key.clone(), data);
                    }
                    if let Some(key) = &self.context_state_key {
                        depot.insert(key.clone(), "authorized");
                    }
                } else {
                    if let Some(key) = &self.context_state_key {
                        depot.insert(key.clone(), "forbidden");
                    }
                    if self.response_error {
                        res.set_http_error(Forbidden());
                    }
                }
                if let Some(key) = &self.context_token_key {
                    depot.insert(key.clone(), token);
                }
                return;
            }
        }
        if let Some(key) = &self.context_state_key {
            depot.insert(key.clone(), "unauthorized");
        }
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
        let auth_handler: JwtHandler<JwtClaims> = JwtHandler::new("ABCDEF".into())
            .with_response_error(true)
            .with_context_token_key(Some("jwt_token".to_owned()))
            .with_context_state_key(Some("jwt_state".to_owned()))
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
            let request = Request::from_hyper(
                hyper::Request::builder()
                    .method("GET")
                    .uri("http://127.0.0.1:7979/hello")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(hyper::Body::empty())
                    .unwrap(),
            );
            service.handle(request).await.take_text().await.unwrap()
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

        let request = Request::from_hyper(
            hyper::Request::builder()
                .method("GET")
                .uri(format!("http://127.0.0.1:7979/hello?jwt_token={}", token))
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let content = service.handle(request).await.take_text().await.unwrap();
        assert!(content.contains("hello"));
        let request = Request::from_hyper(
            hyper::Request::builder()
                .method("GET")
                .uri("http://127.0.0.1:7979/hello")
                .header("Cookie", format!("jwt_token={}", token))
                .body(hyper::Body::empty())
                .unwrap(),
        );
        let content = service.handle(request).await.take_text().await.unwrap();
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
