use jsonwebtoken::{self, EncodingKey};
use salvo::http::{Method, StatusError};
use salvo::jwt_auth::{ConstDecoder, QueryFinder};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

const SECRET_KEY: &str = "YOUR SECRET_KEY";

#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    username: String,
    exp: i64,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let auth_handler: JwtAuth<JwtClaims, _> = JwtAuth::new(ConstDecoder::new(SECRET_KEY))
        .finders(vec![
            // Box::new(HeaderFinder::new()),
            Box::new(QueryFinder::new("jwt_token")),
            // Box::new(CookieFinder::new("jwt_token")),
        ])
        .response_error(false);

    let acceptor = TcpListener::new("127.0.0.1:5800").bind().await;
    Server::new(acceptor)
        .serve(Router::with_hoop(auth_handler).handle(index))
        .await;
}
#[handler]
async fn index(req: &mut Request, depot: &mut Depot, res: &mut Response) -> anyhow::Result<()> {
    if req.method() == Method::POST {
        let (username, password) = (
            req.form::<String>("username").await.unwrap_or_default(),
            req.form::<String>("password").await.unwrap_or_default(),
        );
        if !validate(&username, &password) {
            res.render(Text::Html(LOGIN_HTML));
            return Ok(());
        }
        let exp = OffsetDateTime::now_utc() + Duration::days(14);
        let claim = JwtClaims {
            username,
            exp: exp.unix_timestamp(),
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claim,
            &EncodingKey::from_secret(SECRET_KEY.as_bytes()),
        )?;
        res.render(Redirect::other(format!("/?jwt_token={token}")));
    } else {
        match depot.jwt_auth_state() {
            JwtAuthState::Authorized => {
                let data = depot.jwt_auth_data::<JwtClaims>().unwrap();
                res.render(Text::Plain(format!(
                    "Hi {}, have logged in successfully!",
                    data.claims.username
                )));
            }
            JwtAuthState::Unauthorized => {
                res.render(Text::Html(LOGIN_HTML));
            }
            JwtAuthState::Forbidden => {
                res.render(StatusError::forbidden());
            }
        }
    }
    Ok(())
}

fn validate(username: &str, password: &str) -> bool {
    username == "root" && password == "pwd"
}

static LOGIN_HTML: &str = r#"<!DOCTYPE html>
<html>
    <head>
        <title>JWT Auth Demo</title>
    </head>
    <body>
        <h1>JWT Auth</h1>
        <form action="/" method="post">
        <label for="username"><b>Username</b></label>
        <input type="text" placeholder="Enter Username" name="username" required>
    
        <label for="password"><b>Password</b></label>
        <input type="password" placeholder="Enter Password" name="password" required>
    
        <button type="submit">Login</button>
    </form>
    </body>
</html>
"#;
