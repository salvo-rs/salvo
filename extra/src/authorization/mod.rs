//! basic auth middleware
use salvo_core::async_trait;
use salvo_core::http::header::AUTHORIZATION;
use salvo_core::http::{Request, Response, StatusCode};
use salvo_core::routing::FlowCtrl;
use salvo_core::{Depot, Error, Handler};

/// key used when insert into depot.
pub const DATA_KEY: &str = "::salvo::extra::authorization::data";
/// key used when insert into depot.
pub const DATA_TYPE: &str = "::salvo::extra::authorization::type";

/// AuthorizationValidator
#[async_trait]
pub trait AuthorizationValidator: Send + Sync {
    /// Validate is that username and password is right.
    #[must_use = "validate future must be used"]
    async fn validate(&self, data: AuthorizationResult) -> bool;
}

/// AuthorizationType
#[derive(PartialEq, Debug)]
pub enum AuthorizationType {
    /// Basic Authorization
    Basic,
    /// Bearer Authorization
    Bearer,
    /// Digest Authorization
    Digest,
    /// Any Authorization
    Any,
}

impl ToString for AuthorizationType {
    fn to_string(&self) -> String {
        match self {
            AuthorizationType::Basic => "Basic",
            AuthorizationType::Bearer => "Bearer",
            AuthorizationType::Digest => "Digest",
            // default to Basic
            AuthorizationType::Any => "Basic",
        }
        .to_string()
    }
}

/// AuthorizationResult
pub enum AuthorizationResult {
    /// Basic Authorization
    Basic((String, String)),
    /// Bearer Authorization
    Bearer(String),
    /// Digest Authorization
    Digest(String),
    /// None Authorization
    None,
}

/// AuthorizationHandler
pub struct AuthorizationHandler<V: AuthorizationValidator> {
    auth_type: AuthorizationType,
    realm: String,
    validator: V,
}
impl<V> AuthorizationHandler<V>
where
    V: AuthorizationValidator,
{
    /// Create new `AuthorizationValidator`.
    #[inline]
    pub fn new(auth_type: AuthorizationType, validator: V) -> Self {
        AuthorizationHandler {
            auth_type,
            realm: "realm".to_owned(),
            validator,
        }
    }

    #[inline]
    fn ask_credentials(&self, res: &mut Response) {
        res.headers_mut().insert(
            "WWW-Authenticate",
            format!("{} realm={:?}", self.auth_type.to_string(), self.realm)
                .parse()
                .unwrap(),
        );
        res.set_status_code(StatusCode::UNAUTHORIZED);
    }

    fn parse_basic_authorization<S: AsRef<str>>(&self, authorization: S) -> Result<(String, String), Error> {
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
impl<V> Handler for AuthorizationHandler<V>
where
    V: AuthorizationValidator + 'static,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        if let Some(auth) = req.headers().get(AUTHORIZATION) {
            if let Ok(auth) = auth.to_str() {
                let mut list = auth.split(' ').collect::<Vec<&str>>();

                let auth_type: &str = list.remove(0);
                if self.auth_type.to_string() != auth_type && self.auth_type != AuthorizationType::Any {
                    self.ask_credentials(res);
                    ctrl.skip_rest();
                    return;
                }

                let raw = list.join(" ");
                match auth_type {
                    "Basic" => {
                        if let Ok(u) = self.parse_basic_authorization(raw.clone()) {
                            if self.validator.validate(AuthorizationResult::Basic(u.clone())).await {
                                depot.insert(DATA_KEY, u.0);
                                depot.insert(DATA_TYPE, "Basic");
                                ctrl.call_next(req, depot, res).await;
                                return;
                            }
                        }
                    }
                    "Bearer" => {
                        if self.validator.validate(AuthorizationResult::Bearer(raw.clone())).await {
                            depot.insert(DATA_KEY, raw);
                            depot.insert(DATA_TYPE, "Bearer");
                            ctrl.call_next(req, depot, res).await;
                            return;
                        }
                    }
                    "Digest" => {
                        if self.validator.validate(AuthorizationResult::Digest(raw.clone())).await {
                            depot.insert(DATA_KEY, raw);
                            depot.insert(DATA_TYPE, "Digest");
                            ctrl.call_next(req, depot, res).await;
                            return;
                        }
                    }
                    _ => {}
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

    #[fn_handler]
    async fn hello() -> &'static str {
        "Hello"
    }

    struct Validator;
    #[async_trait]
    impl AuthorizationValidator for Validator {
        async fn validate(&self, data: AuthorizationResult) -> bool {
            if let AuthorizationResult::Basic(msg) = data {
                return &msg.0 == "root" && &msg.1 == "pwd";
            }
            false
        }
    }

    #[tokio::test]
    async fn test_basic_auth() {
        let auth_handler = AuthorizationHandler::new(AuthorizationType::Basic, Validator);
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
