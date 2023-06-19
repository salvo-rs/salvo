use jsonwebtoken::{self, EncodingKey};
use salvo::http::{Method, StatusError};
use salvo::jwt_auth::OidcDecoder;
use salvo::jwt_auth::{CookieFinder, HeaderFinder, QueryFinder};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

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
        .response_error(false);

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    let router = Router::new()
        .push(Router::with_hoop(auth_handler).path("welcome").get(welcome))
        .push(Router::with_path("<**rest>").handle(Proxy::new(vec!["http://localhost:5801"])));
    Server::new(acceptor).serve(router).await;
}
#[handler]
async fn welcome(req: &mut Request, depot: &mut Depot, res: &mut Response) -> Result<String, StatusError> {
    match depot.jwt_auth_state() {
        JwtAuthState::Authorized => {
            let data = depot.jwt_auth_data::<JwtClaims>().unwrap();
            Ok(format!("Hi {}, have logged in successfully!", data.claims.sub))
        }
        JwtAuthState::Unauthorized => {
            Err(StatusError::unauthorized())
        }
        _ => {
            Err(StatusError::forbidden())
        }
    }
}
