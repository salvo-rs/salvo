use anyhow;
use async_trait::async_trait;
use hyper::header::AUTHORIZATION;
use hyper::StatusCode;
use std::sync::Arc;

use salvo_core::depot::Depot;
use salvo_core::http::{Request, Response};
use salvo_core::server::ServerConfig;
use salvo_core::Error;
use salvo_core::Handler;

pub struct BasicAuthHandler {
    config: BasicAuthConfig,
}

pub struct BasicAuthConfig {
    pub realm: String,
    pub context_key: Option<String>,
    pub expires: Option<time::Duration>,
    pub validator: Box<dyn BasicAuthValidator>,
}
pub trait BasicAuthValidator: Send + Sync {
    fn validate(&self, name: String, password: String) -> bool;
}
impl<F> BasicAuthValidator for F
where
    F: Send + Sync,
    F: Fn(String, String) -> bool,
{
    fn validate(&self, name: String, password: String) -> bool {
        self(name, password)
    }
}

impl BasicAuthHandler {
    pub fn new(config: BasicAuthConfig) -> BasicAuthHandler {
        BasicAuthHandler { config }
    }
}
impl BasicAuthHandler {
    fn ask_credentials(&self, res: &mut Response) {
        res.headers_mut()
            .insert("WWW-Authenticate", format!("Basic realm={:?}", self.config.realm).parse().unwrap());
        res.set_status_code(StatusCode::UNAUTHORIZED);
    }
    fn parse_authorization<S: AsRef<str>>(&self, authorization: S) -> Result<(String, String), Error> {
        if let Ok(auth) = base64::decode(authorization.as_ref()) {
            let auth = auth.iter().map(|&c| c as char).collect::<String>();
            let parts: Vec<&str> = auth.splitn(2, ':').collect();
            if parts.len() == 2 {
                Ok((parts[0].to_owned(), parts[1].to_owned()))
            } else {
                Err(anyhow!("error parse auth"))
            }
        } else {
            Err(anyhow!("base64 decode error"))
        }
    }
}
#[async_trait]
impl Handler for BasicAuthHandler {
    async fn handle(&self, _conf: Arc<ServerConfig>, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        if let Some(auth) = req.headers().get(AUTHORIZATION) {
            if let Ok(auth) = auth.to_str() {
                if auth.starts_with("Basic") {
                    if let Some(auth) = auth.splitn(2, ' ').collect::<Vec<&str>>().pop() {
                        if let Ok((user_name, password)) = self.parse_authorization(auth) {
                            if self.config.validator.validate(user_name.clone(), password) {
                                if let Some(key) = &self.config.context_key {
                                    depot.insert(key.clone(), user_name);
                                }
                                return;
                            }
                        }
                    }
                }
            }
        }
        self.ask_credentials(res);
    }
}
