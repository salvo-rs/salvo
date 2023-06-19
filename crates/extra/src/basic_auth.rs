//! basic auth middleware
use salvo_core::http::header::{HeaderName, PROXY_AUTHORIZATION, AUTHORIZATION};
use salvo_core::http::{Request, Response, StatusCode};
use salvo_core::{async_trait, Depot, Error, FlowCtrl, Handler};
use base64::engine::{general_purpose, Engine};

/// key used when insert into depot.
pub const USERNAME_KEY: &str = "::salvo::basic_auth::username";

/// BasicAuthValidator
#[async_trait]
pub trait BasicAuthValidator: Send + Sync {
    /// Validate is that username and password is right.
    async fn validate(&self, username: &str, password: &str, depot: &mut Depot) -> bool;
}
/// BasicAuthDepotExt
pub trait BasicAuthDepotExt {
    /// Get basic auth username reference.
    fn basic_auth_username(&self) -> Option<&String>;
}

impl BasicAuthDepotExt for Depot {
    fn basic_auth_username(&self) -> Option<&String> {
        self.get(USERNAME_KEY)
    }
}

/// BasicAuth
pub struct BasicAuth<V: BasicAuthValidator> {
    realm: String,
    header_names: Vec<HeaderName>,
    validator: V,
}

impl<V> BasicAuth<V>
where
    V: BasicAuthValidator,
{
    /// Create new `BasicAuthValidator`.
    #[inline]
    pub fn new(validator: V) -> Self {
        BasicAuth {
            realm: "realm".to_owned(),
            header_names: vec![AUTHORIZATION, PROXY_AUTHORIZATION],
            validator,
        }
    }

    #[doc(hidden)]
    #[inline]
    pub fn set_header_names(mut self, header_names: impl Into<Vec<HeaderName>>) -> Self {
        self.header_names = header_names.into();
        self
    }
    #[doc(hidden)]
    #[inline]
    pub fn header_names(&self) -> &Vec<HeaderName> {
       & self.header_names
    }

    #[doc(hidden)]
    #[inline]
    pub fn header_names_mut(&mut self) -> &mut Vec<HeaderName> {
       &mut self.header_names
    }

    #[doc(hidden)]
    #[inline]
    pub fn ask_credentials(&self, res: &mut Response) {
        ask_credentials(res, &self.realm)
    }

    #[doc(hidden)]
    #[inline]
    pub fn parse_credentials(&self, req: &Request) -> Result<(String, String), Error> {
        parse_credentials(req, &self.header_names)
    }
}

#[doc(hidden)]
#[inline]
pub fn ask_credentials(res: &mut Response, realm: impl AsRef<str>) {
    res.headers_mut().insert(
        "WWW-Authenticate",
        format!("Basic realm={:?}", realm.as_ref()).parse().unwrap(),
    );
    res.status_code(StatusCode::UNAUTHORIZED);
}

#[doc(hidden)]
#[inline]
pub fn parse_credentials(req: &Request, header_names: &[HeaderName]) -> Result<(String, String), Error> {
    let mut authorization = "";
    for header_name in header_names {
        if let Some(header_value) = req.headers().get(header_name) {
            authorization = header_value.to_str().unwrap_or_default();
            if !authorization.is_empty() {
                break;
            }
        }
    }

    if authorization.starts_with("Basic") {
        if let Some((_, auth)) = authorization.split_once(' ') {
            let auth = general_purpose::STANDARD.decode(auth).map_err(Error::other)?;
            let auth = auth.iter().map(|&c| c as char).collect::<String>();
            if let Some((username, password)) = auth.split_once(':') {
                return Ok((username.to_owned(), password.to_owned()));
            } else {
                return Err(Error::other("`authorization` has bad format"));
            }
        }
    }
    Err(Error::other("parse http header failed"))
}

#[async_trait]
impl<V> Handler for BasicAuth<V>
where
    V: BasicAuthValidator + 'static,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if let Ok((username, password)) = self.parse_credentials(req) {
            if self.validator.validate(&username, &password, depot).await {
                depot.insert(USERNAME_KEY, username);
                ctrl.call_next(req, depot, res).await;
                return;
            }
        }
        self.ask_credentials(res);
        ctrl.skip_rest();
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use super::*;

    #[handler]
    async fn hello() -> &'static str {
        "Hello"
    }

    struct Validator;
    #[async_trait]
    impl BasicAuthValidator for Validator {
        async fn validate(&self, username: &str, password: &str, _depot: &mut Depot) -> bool {
            username == "root" && password == "pwd"
        }
    }

    #[tokio::test]
    async fn test_basic_auth() {
        let auth_handler = BasicAuth::new(Validator);
        let router = Router::with_hoop(auth_handler).handle(hello);
        let service = Service::new(router);

        let content = TestClient::get("http://127.0.0.1:5800/")
            .basic_auth("root", Some("pwd"))
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("Hello"));

        let content = TestClient::get("http://127.0.0.1:5800/")
            .basic_auth("root", Some("pwd2"))
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("Unauthorized"));
    }
}
