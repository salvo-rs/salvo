use hyper::header::AUTHORIZATION;
use jsonwebtoken::{decode, TokenData};
use jsonwebtoken::errors::Error as JwtError;
use serde::de::{DeserializeOwned};
use std::{marker::PhantomData};
use novel::{Context, Handler};
pub use jsonwebtoken::{Validation, Algorithm};

pub struct JwtHandler<C> where C: DeserializeOwned + Sync + Send + 'static{
    config: JwtConfig<C>,
}
pub struct JwtConfig<C> where C: DeserializeOwned + Sync + Send + 'static{
    pub secret: String,
    pub context_claims_key: Option<String>,
    pub context_state_key: Option<String>,
    pub response_error: bool,
    pub claims: PhantomData<C>,
    pub validation: Validation,
    pub extractors: Vec<Box<dyn JwtExtractor>>,
}

pub trait JwtExtractor: Send+Sync{
    fn get_token(&self, ctx: &mut Context) -> Option<String>;
}

#[derive(Default)]
pub struct HeaderExtractor;
impl HeaderExtractor{
    pub fn new()->Self{
        HeaderExtractor{}
    }
}
impl JwtExtractor for HeaderExtractor{
    fn get_token(&self, ctx: &mut Context) -> Option<String>{
        if let Some(auth) = ctx.request().headers().get(AUTHORIZATION){
            if let Ok(auth) = auth.to_str() {
                if auth.starts_with("Bearer") {
                    return auth.splitn(2, ' ').collect::<Vec<&str>>().pop().map(|s|s.to_owned());
                }
            }
        }
        None
    }
}

pub struct FormExtractor(String);
impl FormExtractor{
    pub fn new<T: Into<String>>(name: T) -> Self{
        FormExtractor(name.into())
    }
}
impl JwtExtractor for FormExtractor{
    fn get_token(&self, ctx: &mut Context) -> Option<String>{
        ctx.get_form(&self.0)
    }
}

pub struct QueryExtractor(String);
impl QueryExtractor{
    pub fn new<T: Into<String>>(name: T) -> Self{
        QueryExtractor(name.into())
    }
}
impl JwtExtractor for QueryExtractor{
    fn get_token(&self, ctx: &mut Context) -> Option<String>{
        println!("==========get query: {}, {:#?}", &self.0, ctx.get_query(&self.0));
        ctx.get_query(&self.0)
    }
}

pub struct CookieExtractor(String);
impl CookieExtractor{
    pub fn new<T: Into<String>>(name: T) -> Self{
        CookieExtractor(name.into())
    }
}
impl JwtExtractor for CookieExtractor{
    fn get_token(&self, ctx: &mut Context) -> Option<String>{
        ctx.get_cookie(&self.0).map(|c|c.value().to_owned())
    }
}

impl<C> JwtHandler<C> where C: DeserializeOwned + Sync + Send + 'static {
    pub fn new(config: JwtConfig<C>) -> JwtHandler<C>{
        JwtHandler{
            config,
        }
    }
    pub fn decode(&self, token: &str) -> Result<TokenData<C>, JwtError>{
        decode::<C>(&token, &self.config.secret.as_bytes(), &self.config.validation)
    }
}
impl<C> Handler for JwtHandler<C> where C: DeserializeOwned + Sync + Send + 'static {
    fn handle(&self, ctx: &mut Context){
        println!("==============1,  {}", self.config.extractors.len());
        for extractor in &self.config.extractors {
            println!("==============...2");
           if let Some(token) = extractor.get_token(ctx) {
            println!("==============2");
                if let Ok(claims) = self.decode(&token){
                    println!("==============3");
                    if let Some(key) = &self.config.context_claims_key {
                        println!("==============4{}", key);
                        ctx.depot_mut().insert(key.clone(), claims);
                    }
                }else{
                    if self.config.response_error {
                        ctx.forbidden();
                    } else if let Some(key) = &self.config.context_state_key {
                        ctx.depot_mut().insert(key.clone(), "forbidden");
                    }
                }
                return;
           }
        }
       if self.config.response_error {
           ctx.unauthorized();
       }  else if let Some(key) = &self.config.context_state_key {
           ctx.depot_mut().insert(key.clone(), "unauthorized");
       }
    }
}