//! Middleware for HTTP Basic Authentication.
//!
//! This middleware implements the standard HTTP Basic Authentication scheme as described in RFC 7617.
//! It extracts credentials from the Authorization header and validates them against your custom validator.
//!
//! # Example
//!
//! ```no_run
//! use salvo_core::prelude::*;
//! use salvo_extra::basic_auth::{BasicAuth, BasicAuthValidator};
//!
//! struct Validator;
//! impl BasicAuthValidator for Validator {
//!     async fn validate(&self, username: &str, password: &str, _depot: &mut Depot) -> bool {
//!         username == "root" && password == "pwd"
//!     }
//! }
//! 
//! #[handler]
//! async fn hello() -> &'static str {
//!     "Hello"
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let auth_handler = BasicAuth::new(Validator);
//!     let router = Router::with_hoop(auth_handler).goal(hello);
//!
//!     let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
//!     Server::new(acceptor).serve(router).await;
//! }
//! ```
use base64::engine::{general_purpose, Engine};
use salvo_core::http::header::{HeaderName, AUTHORIZATION, PROXY_AUTHORIZATION};
use salvo_core::http::{Request, Response, StatusCode};
use salvo_core::{async_trait, Depot, Error, FlowCtrl, Handler};

/// key used when insert into depot.
pub const USERNAME_KEY: &str = "::salvo::basic_auth::username";

/// Validator for Basic Authentication credentials.
pub trait BasicAuthValidator: Send + Sync {
    /// Validates whether the provided username and password are correct.
    /// 
    /// Implement this method to check credentials against your authentication system.
    /// Return `true` if authentication succeeds, `false` otherwise.
    fn validate(&self, username: &str, password: &str, depot: &mut Depot) -> impl Future<Output = bool> + Send;
}

/// Extension trait for retrieving the authenticated username from a Depot.
pub trait BasicAuthDepotExt {
    /// Returns the authenticated username if authentication was successful.
    fn basic_auth_username(&self) -> Option<&str>;
}

impl BasicAuthDepotExt for Depot {
    fn basic_auth_username(&self) -> Option<&str> {
        self.get::<String>(USERNAME_KEY).map(|v|&**v).ok()
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
        &self.header_names
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
        format!("Basic realm={:?}", realm.as_ref())
            .parse()
            .expect("parse WWW-Authenticate failed"),
    );
    res.status_code(StatusCode::UNAUTHORIZED);
}

#[doc(hidden)]
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
    impl BasicAuthValidator for Validator {
        async fn validate(&self, username: &str, password: &str, _depot: &mut Depot) -> bool {
            username == "root" && password == "pwd"
        }
    }

    #[tokio::test]
    async fn test_basic_auth() {
        let auth_handler = BasicAuth::new(Validator);
        let router = Router::with_hoop(auth_handler).goal(hello);
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
