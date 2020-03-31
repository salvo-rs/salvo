use async_trait::async_trait;
use hyper::header::AUTHORIZATION;
pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::{decode, Algorithm, TokenData, Validation};
use serde::de::DeserializeOwned;
use std::marker::PhantomData;

use salvo_core::depot::Depot;
use salvo_core::http::{Request, Response};
use salvo_core::server::ServerConfig;
use salvo_core::Handler;
use std::sync::Arc;

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
        decode::<C>(&token, &self.config.secret.as_bytes(), &self.config.validation)
    }
}

#[async_trait]
impl<C> Handler for JwtHandler<C>
where
    C: DeserializeOwned + Sync + Send + 'static,
{
    async fn handle(&self, _conf: Arc<ServerConfig>, req: &mut Request, depot: &mut Depot, resp: &mut Response) {
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
                        resp.forbidden();
                    }
                }
                return;
            }
        }
        if let Some(key) = &self.config.context_state_key {
            depot.insert(key.clone(), "unauthorized");
        }
        if self.config.response_error {
            resp.unauthorized();
        }
    }
}
