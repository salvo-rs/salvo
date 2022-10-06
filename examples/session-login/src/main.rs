use salvo::prelude::*;
use salvo::session::{CookieStore, Session, SessionDepotExt, SessionHandler};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    let session_handler = SessionHandler::builder(
        CookieStore::new(),
        b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
    )
    .build()
    .unwrap();
    let router = Router::new()
        .hoop(session_handler)
        .get(home)
        .push(Router::with_path("login").get(login).post(login))
        .push(Router::with_path("logout").get(logout));
    tracing::info!("Listening on http://127.0.0.1:7878");
    Server::new(TcpListener::bind("127.0.0.1:7878")).serve(router).await;
}

#[handler]
pub async fn login(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    if req.method() == salvo::http::Method::POST {
        let mut session = Session::new();
        session
            .insert("username", req.form::<String>("username").await.unwrap())
            .unwrap();
        depot.set_session(session);
        res.render(Redirect::other("/"));
    } else {
        res.render(Text::Html(LOGIN_HTML));
    }
}

#[handler]
pub async fn logout(depot: &mut Depot, res: &mut Response) {
    if let Some(session) = depot.session_mut() {
        session.remove("username");
    }
    res.render(Redirect::other("/"));
}

#[handler]
pub async fn home(depot: &mut Depot, res: &mut Response) {
    let mut content = r#"<a href="login">Login</h1>"#.into();
    if let Some(session) = depot.session_mut() {
        if let Some(username) = session.get::<String>("username") {
            content = format!(r#"Hello, {}. <br><a href="logout">Logout</h1>"#, username);
        }
    }
    res.render(Text::Html(content));
}

static LOGIN_HTML: &str = r#"<!DOCTYPE html>
<html>
    <head>
        <title>Login</title>
    </head>
    <body>
        <form action="/login" method="post">
            <h1>Login</h1>
            <input type="text" name="username" />
            <button type="submit" id="submit">Submit</button>
        </form>
    </body>
</html>
"#;
