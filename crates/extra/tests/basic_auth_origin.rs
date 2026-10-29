#![cfg(feature = "basic-auth")]

use salvo_core::http::header::{AUTHORIZATION, PROXY_AUTHORIZATION};
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_extra::basic_auth::{BasicAuth, BasicAuthDepotExt, BasicAuthValidator};

struct Validator;

impl BasicAuthValidator for Validator {
    async fn validate(&self, username: &str, password: &str, _depot: &mut Depot) -> bool {
        username == "alice" && password == "origin-secret"
    }
}

#[handler]
async fn identity(depot: &Depot) -> String {
    depot.basic_auth_username().unwrap().to_owned()
}

#[tokio::test]
async fn default_basic_auth_does_not_accept_proxy_credentials() {
    let service = Service::new(Router::new().hoop(BasicAuth::new(Validator)).get(identity));

    for authorization in [None, Some("Bearer unrelated")] {
        let mut request = TestClient::get("http://localhost/").add_header(
            PROXY_AUTHORIZATION,
            "Basic YWxpY2U6b3JpZ2luLXNlY3JldA==",
            true,
        );
        if let Some(authorization) = authorization {
            request = request.add_header(AUTHORIZATION, authorization, true);
        }
        let res = request.send(&service).await;
        assert_eq!(res.status_code, Some(StatusCode::UNAUTHORIZED));
        assert!(res.headers().contains_key("www-authenticate"));
    }

    let mut res = TestClient::get("http://localhost/")
        .basic_auth("alice", Some("origin-secret"))
        .send(&service)
        .await;
    assert_eq!(res.status_code, Some(StatusCode::OK));
    assert_eq!(res.take_string().await.unwrap(), "alice");
}

#[tokio::test]
async fn proxy_credentials_remain_available_with_explicit_configuration() {
    let service = Service::new(
        Router::new()
            .hoop(BasicAuth::new(Validator).set_header_names([AUTHORIZATION, PROXY_AUTHORIZATION]))
            .get(identity),
    );
    let mut res = TestClient::get("http://localhost/")
        .add_header(AUTHORIZATION, "Bearer unrelated", true)
        .add_header(
            PROXY_AUTHORIZATION,
            "Basic YWxpY2U6b3JpZ2luLXNlY3JldA==",
            true,
        )
        .send(&service)
        .await;
    assert_eq!(res.status_code, Some(StatusCode::OK));
    assert_eq!(res.take_string().await.unwrap(), "alice");
}
