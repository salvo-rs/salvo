use async_trait::async_trait;
pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::{decode, Algorithm, DecodingKey, TokenData, Validation};
use serde::de::DeserializeOwned;
use std::marker::PhantomData;

use salvo_core::http::errors::*;
use salvo_core::http::header::AUTHORIZATION;
use salvo_core::http::{Request, Response};
use salvo_core::Depot;
use salvo_core::Handler;

#[async_trait]
pub trait JwtExtractor: Send + Sync {
    async fn get_token(&self, req: &mut Request) -> Option<String>;
}

#[derive(Default)]
pub struct HeaderExtractor;
impl HeaderExtractor {
    pub fn new() -> Self {
        HeaderExtractor {}
    }
}
#[async_trait]
impl JwtExtractor for HeaderExtractor {
    async fn get_token(&self, req: &mut Request) -> Option<String> {
        if let Some(auth) = req.headers().get(AUTHORIZATION) {
            if let Ok(auth) = auth.to_str() {
                if auth.starts_with("Bearer") {
                    return auth.splitn(2, ' ').collect::<Vec<&str>>().pop().map(|s| s.to_owned());
                }
            }
        }
        None
    }
}

pub struct FormExtractor(String);
impl FormExtractor {
    pub fn new<T: Into<String>>(name: T) -> Self {
        FormExtractor(name.into())
    }
}
#[async_trait]
impl JwtExtractor for FormExtractor {
    async fn get_token(&self, req: &mut Request) -> Option<String> {
        req.get_form(&self.0).await
    }
}

pub struct QueryExtractor(String);
impl QueryExtractor {
    pub fn new<T: Into<String>>(name: T) -> Self {
        QueryExtractor(name.into())
    }
}

#[async_trait]
impl JwtExtractor for QueryExtractor {
    async fn get_token(&self, req: &mut Request) -> Option<String> {
        req.get_query(&self.0)
    }
}

pub struct CookieExtractor(String);
impl CookieExtractor {
    pub fn new<T: Into<String>>(name: T) -> Self {
        CookieExtractor(name.into())
    }
}
#[async_trait]
impl JwtExtractor for CookieExtractor {
    async fn get_token(&self, req: &mut Request) -> Option<String> {
        req.get_cookie(&self.0).map(|c| c.value().to_owned())
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
    pub fn response_error(&self) -> bool {
        self.response_error
    }
    pub fn with_response_error(mut self, response_error: bool) -> Self {
        self.response_error = response_error;
        self
    }

    pub fn secret(&self) -> &String {
        &self.secret
    }
    pub fn secret_mut(&mut self) -> &mut String {
        &mut self.secret
    }
    pub fn with_secret(mut self, secret: String) -> Self {
        self.secret = secret;
        self
    }
    pub fn context_token_key(&self) -> Option<&String> {
        self.context_token_key.as_ref()
    }
    pub fn with_context_token_key(mut self, context_token_key: Option<String>) -> Self {
        self.context_token_key = context_token_key;
        self
    }
    pub fn context_data_key(&self) -> Option<&String> {
        self.context_data_key.as_ref()
    }
    pub fn with_context_data_key(mut self, context_data_key: Option<String>) -> Self {
        self.context_data_key = context_data_key;
        self
    }
    pub fn context_state_key(&self) -> Option<&String> {
        self.context_state_key.as_ref()
    }
    pub fn with_context_state_key(mut self, context_state_key: Option<String>) -> Self {
        self.context_state_key = context_state_key;
        self
    }
    pub fn extractors(&self) -> &Vec<Box<dyn JwtExtractor>> {
        &self.extractors
    }
    pub fn extractors_mut(&mut self) -> &mut Vec<Box<dyn JwtExtractor>> {
        &mut self.extractors
    }
    pub fn with_extractors(mut self, extractors: Vec<Box<dyn JwtExtractor>>) -> Self {
        self.extractors = extractors;
        self
    }

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
