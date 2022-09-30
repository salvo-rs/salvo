//! basic auth middleware
use salvo_core::http::header::AUTHORIZATION;
use salvo_core::http::{Request, Response, StatusCode};
use salvo_core::{async_trait, Depot, Error, Handler, FlowCtrl};

/// key used when insert into depot.
pub const USERNAME_KEY: &str = "::salvo::basic_auth::username";

/// BasicAuthValidator
#[async_trait]
pub trait BasicAuthValidator: Send + Sync {
    /// Validate is that username and password is right.
    #[must_use = "validate future must be used"]
    async fn validate(&self, username: &str, password: &str) -> bool;
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
            validator,
        }
    }

    #[inline]
    fn ask_credentials(&self, res: &mut Response) {
        res.headers_mut().insert(
            "WWW-Authenticate",
            format!("Basic realm={:?}", self.realm).parse().unwrap(),
        );
        res.set_status_code(StatusCode::UNAUTHORIZED);
    }

    #[inline]
    fn parse_authorization<S: AsRef<str>>(&self, authorization: S) -> Result<(String, String), Error> {
        let auth = base64::decode(authorization.as_ref()).map_err(Error::other)?;
        let auth = auth.iter().map(|&c| c as char).collect::<String>();
        if let Some((username, password)) = auth.split_once(':') {
            Ok((username.to_owned(), password.to_owned()))
        } else {
            Err(Error::other("parse http header failed"))
        }
    }
}

#[async_trait]
impl<V> Handler for BasicAuth<V>
where
    V: BasicAuthValidator + 'static,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if let Some(auth) = req.headers().get(AUTHORIZATION).and_then(|auth| auth.to_str().ok()) {
            if auth.starts_with("Basic") {
                if let Some((_, auth)) = auth.split_once(' ') {
                    if let Ok((username, password)) = self.parse_authorization(auth) {
                        if self.validator.validate(&username, &password).await {
                            depot.insert(USERNAME_KEY, username);
                            ctrl.call_next(req, depot, res).await;
                            return;
                        }
                    }
                }
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
        async fn validate(&self, username: &str, password: &str) -> bool {
            username == "root" && password == "pwd"
        }
    }

    #[tokio::test]
    async fn test_basic_auth() {
        let auth_handler = BasicAuth::new(Validator);
        let router = Router::with_hoop(auth_handler).handle(hello);
        let service = Service::new(router);

        let content = TestClient::get("http://127.0.0.1:7878/")
            .basic_auth("root", Some("pwd"))
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("Hello"));

        let content = TestClient::get("http://127.0.0.1:7878/")
            .basic_auth("root", Some("pwd2"))
            .send(&service)
            .await
            .take_string()
            .await
            .unwrap();
        assert!(content.contains("Unauthorized"));
    }
}
