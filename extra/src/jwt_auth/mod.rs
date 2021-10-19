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

pub struct JwtHandler<C>
where
    C: DeserializeOwned + Sync + Send + 'static,
{
    config: JwtConfig<C>,
}
pub struct JwtConfig<C>
where
    C: DeserializeOwned + Sync + Send + 'static,
{
    pub secret: String,
    pub context_token_key: Option<String>,
    pub context_data_key: Option<String>,
    pub context_state_key: Option<String>,
    pub response_error: bool,
    pub claims: PhantomData<C>,
    pub validation: Validation,
    pub extractors: Vec<Box<dyn JwtExtractor>>,
}
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

impl<C> JwtHandler<C>
where
    C: DeserializeOwned + Sync + Send + 'static,
{
    pub fn new(config: JwtConfig<C>) -> JwtHandler<C> {
        JwtHandler { config }
    }
    pub fn decode(&self, token: &str) -> Result<TokenData<C>, JwtError> {
        decode::<C>(
            token,
            &DecodingKey::from_secret(&*self.config.secret.as_ref()),
            &self.config.validation,
        )
    }
}

#[async_trait]
impl<C> Handler for JwtHandler<C>
where
    C: DeserializeOwned + Sync + Send + 'static,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        for extractor in &self.config.extractors {
            if let Some(token) = extractor.get_token(req).await {
                if let Ok(data) = self.decode(&token) {
                    if let Some(key) = &self.config.context_data_key {
                        depot.insert(key.clone(), data);
                    }
                    if let Some(key) = &self.config.context_state_key {
                        depot.insert(key.clone(), "authorized");
                    }
                } else {
                    if let Some(key) = &self.config.context_state_key {
                        depot.insert(key.clone(), "forbidden");
                    }
                    if self.config.response_error {
                        res.set_http_error(Forbidden());
                    }
                }
                if let Some(key) = &self.config.context_token_key {
                    depot.insert(key.clone(), token);
                }
                return;
            }
        }
        if let Some(key) = &self.config.context_state_key {
            depot.insert(key.clone(), "unauthorized");
        }
        if self.config.response_error {
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
        let baconfig = JwtConfig {
            response_error: true,
            context_token_key: Some("jwt_token".to_owned()),
            context_data_key: Some("jwt_data".to_owned()),
            context_state_key: Some("jwt_state".to_owned()),
            secret: "ABCDEF".into(),
            claims: PhantomData::<JwtClaims>,
            extractors: vec![Box::new(HeaderExtractor::new())],
            validation: Validation::default(),
        };
        let auth_handler = JwtHandler::new(baconfig);

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
