//! Session cookie names must retain their exact wire identity.
use salvo_core::http::header::{COOKIE, SET_COOKIE};
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_session::{MemoryStore, SessionDepotExt, SessionHandler};

#[handler]
async fn login(req: &mut Request, depot: &mut Depot) {
    let user = req.query::<String>("user").unwrap();
    let session = depot.session_mut().unwrap();
    session.regenerate();
    session.insert("user", user).unwrap();
}

#[handler]
async fn whoami(depot: &mut Depot) -> String {
    depot
        .session()
        .unwrap()
        .get::<String>("user")
        .unwrap_or_default()
}

fn pair(response: &Response) -> String {
    response.headers()[SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn encoded_alias_cannot_select_another_valid_session() {
    let sessions = SessionHandler::builder(MemoryStore::new(), &[42; 64])
        .cookie_name("__Host-session")
        .secure_cookie(true)
        .build()
        .unwrap();
    let service = Service::new(
        Router::new()
            .hoop(sessions)
            .push(Router::with_path("login").post(login))
            .push(Router::with_path("me").get(whoami)),
    );
    let attacker = pair(
        &TestClient::post("https://app.example.test/login?user=attacker")
            .send(&service)
            .await,
    );
    let victim = pair(
        &TestClient::post("https://app.example.test/login?user=victim")
            .send(&service)
            .await,
    );
    let alias = attacker.replacen("__Host-session=", "%5F_Host-session=", 1);
    for (header, expected) in [
        (alias.clone(), ""),
        (format!("{victim}; {alias}"), "victim"),
    ] {
        let mut response = TestClient::get("https://app.example.test/me")
            .add_header(COOKIE, header, true)
            .send(&service)
            .await;
        assert_eq!(
            response.take_string().await.unwrap(),
            expected,
            "An encoded cookie name must not replace the authenticated cookie"
        );
    }
}
