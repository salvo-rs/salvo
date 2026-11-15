//! Audit evidence for generic decoder error exposure and Bearer challenges.
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_jwt_auth::{ConstDecoder, JWT_AUTH_ERROR_KEY, JwtAuth, JwtAuthDecoder, JwtAuthDepotExt};
use serde::Deserialize;

#[derive(Clone, Deserialize)]
struct Claims {}
struct CustomDecoder;
impl JwtAuthDecoder for CustomDecoder {
    type Error = std::io::Error;
    async fn decode<C>(
        &self,
        _: &str,
        _: &mut Depot,
    ) -> Result<salvo_jwt_auth::TokenData<C>, Self::Error>
    where
        C: for<'de> Deserialize<'de> + Clone,
    {
        Err(std::io::Error::other("provider is unavailable"))
    }
}

#[handler]
async fn inspect(depot: &mut Depot) -> String {
    format!(
        "typed={} public={}",
        depot.get::<std::io::Error>(JWT_AUTH_ERROR_KEY).is_ok(),
        depot.jwt_auth_error_typed::<std::io::Error>().is_some()
    )
}

#[tokio::test]
async fn custom_decoder_error_is_available_from_the_typed_accessor() {
    let auth: JwtAuth<Claims, _> = JwtAuth::new(CustomDecoder).force_passed(true);
    let service = Service::new(Router::new().hoop(auth).goal(inspect));
    let mut response = TestClient::get("http://localhost/")
        .add_header("authorization", "Bearer token", true)
        .send(&service)
        .await;
    assert_eq!(
        response.take_string().await.unwrap(),
        "typed=true public=true"
    );
}

#[tokio::test]
async fn missing_bearer_credential_returns_a_challenge() {
    let auth: JwtAuth<Claims, _> =
        JwtAuth::new(ConstDecoder::from_base64_secret("c2VjcmV0").unwrap());
    let service = Service::new(Router::new().hoop(auth).goal(inspect));
    let response = TestClient::get("http://localhost/").send(&service).await;
    assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
    assert_eq!(response.headers()["www-authenticate"], "Bearer");
}

#[tokio::test]
async fn invalid_bearer_credential_requests_reauthentication() {
    let auth: JwtAuth<Claims, _> =
        JwtAuth::new(ConstDecoder::from_base64_secret("c2VjcmV0").unwrap());
    let service = Service::new(Router::new().hoop(auth).goal(inspect));
    let response = TestClient::get("http://localhost/")
        .add_header("authorization", "Bearer invalid", true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
    assert_eq!(
        response.headers()["www-authenticate"],
        "Bearer error=\"invalid_token\""
    );
}

#[cfg(feature = "oidc")]
#[test]
fn jwk_store_constructor_indexes_supplied_keys() {
    use jsonwebtoken::jwk::JwkSet;
    use jsonwebtoken::{Algorithm, Validation};
    use salvo_jwt_auth::oidc::{CachePolicy, JwkSetStore};
    let jwks: JwkSet = serde_json::from_value(serde_json::json!({"keys":[{
        "kty":"RSA", "kid":"present", "alg":"RS256", "n":"AQID", "e":"AQAB"
    }]}))
    .unwrap();
    let store = JwkSetStore::new(
        jwks,
        CachePolicy::default(),
        Validation::new(Algorithm::RS256),
    );
    assert_eq!(store.jwks().keys.len(), 1);
    assert!(store.get_key("present").is_some());
}

#[cfg(feature = "oidc")]
#[test]
fn first_max_age_is_not_extended_by_a_duplicate() {
    use salvo_core::http::HeaderValue;
    use salvo_jwt_auth::oidc::CachePolicy;
    let fields = [
        HeaderValue::from_static("max-age=0"),
        HeaderValue::from_static("max-age=2592000"),
    ];
    assert_eq!(
        CachePolicy::from_header_values(&fields).max_age,
        std::time::Duration::ZERO
    );
}
