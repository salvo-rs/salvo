use salvo::http::StatusError;
use salvo::jwt_auth::{HeaderFinder, OidcDecoder};
use salvo::prelude::*;
use salvo::proxy::HyperClient;
use serde::{Deserialize, Serialize};

const ISSUER_URL: &str = "https://coherent-gopher-0.clerk.accounts.dev";

#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    sid: String,
    sub: String,
    exp: i64,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let decoder = OidcDecoder::new(ISSUER_URL.to_owned()).await.unwrap();
    let auth_handler: JwtAuth<JwtClaims, OidcDecoder> = JwtAuth::new(decoder)
        .finders(vec![Box::new(HeaderFinder::new())])
        .force_passed(true);

    let acceptor = TcpListener::new("0.0.0.0:5800").bind().await;
    let router = Router::new()
        .push(Router::with_hoop(auth_handler).path("welcome").get(welcome))
        .push(Router::with_path("{**rest}").goal(Proxy::new(
            vec!["http://localhost:5801"],
            HyperClient::default(),
        )));
    Server::new(acceptor).serve(router).await;
}
#[handler]
async fn welcome(depot: &mut Depot) -> Result<String, StatusError> {
    match depot.jwt_auth_state() {
        JwtAuthState::Authorized => {
            let data = depot.jwt_auth_data::<JwtClaims>().unwrap();
            Ok(format!(
                "Hi {}, have logged in successfully!",
                data.claims.sub
            ))
        }
        JwtAuthState::Unauthorized => Err(StatusError::unauthorized()),
        _ => Err(StatusError::forbidden()),
    }
}
