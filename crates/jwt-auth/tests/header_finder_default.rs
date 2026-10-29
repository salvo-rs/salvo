use jsonwebtoken::{EncodingKey, Header};
use salvo_core::http::Method;
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_jwt_auth::{
    ConstDecoder, HeaderFinder, JwtAuth, JwtAuthDepotExt, install_crypto_provider,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
struct Claims {
    sub: String,
    exp: u64,
}

#[handler]
async fn identity(depot: &mut Depot) -> String {
    depot.jwt_auth_data::<Claims>().unwrap().claims.sub.clone()
}

#[tokio::test]
async fn default_header_finder_authenticates_like_new() {
    let _ = install_crypto_provider();
    let token = jsonwebtoken::encode(
        &Header::default(),
        &Claims {
            sub: "alice".into(),
            exp: u64::MAX,
        },
        &EncodingKey::from_secret(b"test-secret"),
    )
    .unwrap();

    for finder in [HeaderFinder::new(), HeaderFinder::default()] {
        let auth: JwtAuth<Claims, _> =
            JwtAuth::new(ConstDecoder::from_secret(b"test-secret")).finders(vec![Box::new(finder)]);
        let service = Service::new(Router::new().hoop(auth).goal(identity));
        for method in [Method::GET, Method::POST, Method::QUERY] {
            let mut response = salvo_core::test::RequestBuilder::new("http://localhost/", method)
                .add_header("authorization", format!("Bearer {token}"), true)
                .send(&service)
                .await;
            assert_eq!(response.status_code, Some(StatusCode::OK));
            assert_eq!(response.take_string().await.unwrap(), "alice");
        }
        let response = TestClient::get("http://localhost/").send(&service).await;
        assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
        let response = TestClient::get("http://localhost/")
            .add_header("proxy-authorization", format!("Bearer {token}"), true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));
    }
}
