use chrono::{Duration, Utc};
use jsonwebtoken::{self, EncodingKey};
use salvo::anyhow;
use salvo::extra::jwt_auth::{JwtAuthDepotExt, JwtAuthHandler, JwtAuthState, QueryExtractor};
use salvo::http::errors::*;
use salvo::http::Method;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};

const SECRET_KEY: &str = "YOUR SECRET_KEY";

#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    username: String,
    exp: i64,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let auth_handler: JwtAuthHandler<JwtClaims> = JwtAuthHandler::new(SECRET_KEY.to_owned())
        .with_extractors(vec![
            // Box::new(HeaderExtractor::new()),
            Box::new(QueryExtractor::new("jwt_token")),
            // Box::new(CookieExtractor::new("jwt_token")),
        ])
        .with_response_error(false);
    tracing::info!("Listening on http://0.0.0.0:7878");
    Server::new(TcpListener::bind("0.0.0.0:7878"))
        .serve(Router::with_hoop(auth_handler).handle(index))
        .await;
}
#[fn_handler]
async fn index(req: &mut Request, depot: &mut Depot, res: &mut Response) -> anyhow::Result<()> {
    if req.method() == Method::POST {
        let (username, password) = (
            req.get_form::<String>("username").await.unwrap_or_default(),
            req.get_form::<String>("password").await.unwrap_or_default(),
        );
        if !validate(&username, &password) {
            res.render(Text::Html(LOGIN_HTML));
            return Ok(());
        }
        let exp = Utc::now() + Duration::days(14);
        let claim = JwtClaims {
            username,
            exp: exp.timestamp(),
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claim,
            &EncodingKey::from_secret(SECRET_KEY.as_bytes()),
        )?;
        res.redirect_other(&format!("/?jwt_token={}", token))?;
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
                res.set_status_error(Forbidden());
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
