//! Encoded cookie names must not alias a configured authentication cookie.
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_jwt_auth::{ConstDecoder, CookieFinder, JwtAuth, JwtAuthDepotExt};

#[handler]
async fn identity(depot: &mut Depot) -> String {
    depot.jwt_auth_data::<serde_json::Value>().unwrap().claims["sub"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn encoded_host_cookie_alias_cannot_select_another_jwt() {
    let _ = salvo_jwt_auth::install_crypto_provider();
    let secret = "YXVkaXQtbG9jYWwtc2VjcmV0LXdpdGgtc3VmZmljaWVudC1lbnRyb3B5LWZvci1oczI1Ng==";
    let issue = |sub: &str| {
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            &serde_json::json!({"sub": sub, "exp": 4000000000u64}),
            &jsonwebtoken::EncodingKey::from_base64_secret(secret).unwrap(),
        )
        .unwrap()
    };
    let victim = issue("victim");
    let attacker = issue("attacker");
    let auth: JwtAuth<serde_json::Value, _> =
        JwtAuth::new(ConstDecoder::from_base64_secret(secret).unwrap())
            .finders(vec![Box::new(CookieFinder::new("__Host-token"))]);
    let service = Service::new(Router::new().hoop(auth).goal(identity));
    for (cookie, expected) in [
        (format!("__%48ost-token={attacker}"), None),
        (
            format!("__Host-token={victim}; __%48ost-token={attacker}"),
            Some("victim"),
        ),
    ] {
        let mut response = TestClient::get("https://app.example.test/")
            .add_header("cookie", cookie, true)
            .send(&service)
            .await;
        if let Some(expected) = expected {
            assert_eq!(response.status_code, Some(StatusCode::OK));
            assert_eq!(response.take_string().await.unwrap(), expected);
        } else {
            assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
        }
    }
}
