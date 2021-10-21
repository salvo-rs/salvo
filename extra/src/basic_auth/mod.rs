use async_trait::async_trait;

use salvo_core::http::header::AUTHORIZATION;
use salvo_core::http::{Request, Response, StatusCode};
use salvo_core::Depot;
use salvo_core::Handler;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Base64 decode error.")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("Parse http header error")]
    ParseHttpHeader,
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

pub struct BasicAuthHandler<V: BasicAuthValidator> {
    realm: String,
    context_key: Option<String>,
    validator: V,
}
impl<V> BasicAuthHandler<V>
where
    V: BasicAuthValidator,
{
    pub fn new(realm: String, validator: V) -> Self {
        BasicAuthHandler {
            realm,
            context_key: None,
            validator,
        }
    }
    #[inline]
    pub fn context_key(&self) -> Option<&String> {
        self.context_key.as_ref()
    }
    #[inline]
    pub fn set_context_key(&mut self, context_key: Option<String>) {
        self.context_key = context_key;
    }
    #[inline]
    pub fn with_context_key(mut self, context_key: Option<String>) -> Self {
        self.context_key = context_key;
        self
    }

    #[inline]
    fn ask_credentials(&self, res: &mut Response) {
        res.headers_mut().insert(
            "WWW-Authenticate",
            format!("Basic realm={:?}", self.realm).parse().unwrap(),
        );
        res.set_status_code(StatusCode::UNAUTHORIZED);
    }

    fn parse_authorization<S: AsRef<str>>(&self, authorization: S) -> Result<(String, String), Error> {
        let auth = base64::decode(authorization.as_ref())?;
        let auth = auth.iter().map(|&c| c as char).collect::<String>();
        let parts: Vec<&str> = auth.splitn(2, ':').collect();
        if parts.len() == 2 {
            Ok((parts[0].to_owned(), parts[1].to_owned()))
        } else {
            Err(Error::ParseHttpHeader)
        }
    }
}
#[async_trait]
impl<V> Handler for BasicAuthHandler<V>
where
    V: BasicAuthValidator + 'static,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response) {
        if let Some(auth) = req.headers().get(AUTHORIZATION) {
            if let Ok(auth) = auth.to_str() {
                if auth.starts_with("Basic") {
                    if let Some(auth) = auth.splitn(2, ' ').collect::<Vec<&str>>().pop() {
                        if let Ok((user_name, password)) = self.parse_authorization(auth) {
                            if self.validator.validate(user_name.clone(), password) {
                                if let Some(key) = &self.context_key {
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

#[cfg(test)]
mod tests {
    use salvo_core::http::headers::{Authorization, HeaderMapExt};
    use salvo_core::hyper;
    use salvo_core::prelude::*;

    use super::*;

    #[tokio::test]
    async fn test_basic_auth() {
        let auth_handler = BasicAuthHandler::new("realm".to_owned(), |user_name, password| -> bool {
            user_name == "root" && password == "pwd"
        })
        .with_context_key(Some("user_name".to_owned()));

        #[fn_handler]
        async fn hello() -> &'static str {
            "hello"
        }

        let router = Router::new()
            .before(auth_handler)
            .push(Router::with_path("hello").get(hello));
        let service = Service::new(router);

        let mut request = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/hello");
        let headers = request.headers_mut().unwrap();
        headers.typed_insert(Authorization::basic("root", "pwd"));
        let request = Request::from_hyper(request.body(hyper::Body::empty()).unwrap());
        let content = service.handle(request).await.take_text().await.unwrap();
        assert!(content.contains("hello"));

        let mut request = hyper::Request::builder()
            .method("GET")
            .uri("http://127.0.0.1:7979/hello");
        let headers = request.headers_mut().unwrap();
        headers.typed_insert(Authorization::basic("root", "pwd2"));
        let request = Request::from_hyper(request.body(hyper::Body::empty()).unwrap());
        let content = service.handle(request).await.take_text().await.unwrap();
        assert!(content.contains("Unauthorized"));
    }
}
