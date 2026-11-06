//! HTTP regressions for session rotation, logout, and stale concurrent requests.

use std::sync::Arc;

use salvo_core::http::header::{COOKIE, SET_COOKIE};
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_session::{MemoryStore, SessionDepotExt, SessionHandler};
use tokio::sync::Notify;

#[handler]
async fn login(depot: &mut Depot) {
    let session = depot.session_mut().expect("session middleware ran");
    session.regenerate();
    session
        .insert("user", "victim")
        .expect("serialize identity");
}

#[handler]
async fn rotate(depot: &mut Depot) {
    depot
        .session_mut()
        .expect("session middleware ran")
        .regenerate();
}

#[handler]
async fn logout(depot: &mut Depot) {
    depot
        .session_mut()
        .expect("session middleware ran")
        .destroy();
}

#[handler]
async fn whoami(depot: &mut Depot) -> String {
    depot
        .session()
        .expect("session middleware ran")
        .get::<String>("user")
        .unwrap_or_else(|| "anonymous".into())
}

struct DelayedWrite {
    entered: Arc<Notify>,
    resume: Arc<Notify>,
}

#[async_trait]
impl Handler for DelayedWrite {
    async fn handle(
        &self,
        _req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        self.entered.notify_one();
        self.resume.notified().await;
        depot
            .session_mut()
            .expect("session middleware ran")
            .insert("late", true)
            .expect("serialize late write");
        res.render("late write");
    }
}

fn service(save_unchanged: bool, delayed: Option<DelayedWrite>) -> Service {
    let handler = SessionHandler::builder(MemoryStore::new(), &[42; 64])
        .session_ttl(None)
        .save_unchanged(save_unchanged)
        .build()
        .expect("valid session configuration");
    let mut router = Router::new()
        .hoop(handler)
        .push(Router::with_path("login").post(login))
        .push(Router::with_path("rotate").post(rotate))
        .push(Router::with_path("logout").post(logout))
        .push(Router::with_path("me").get(whoami));
    if let Some(delayed) = delayed {
        router = router.push(Router::with_path("late").post(delayed));
    }
    Service::new(router)
}

fn session_cookie(response: &Response) -> String {
    response.headers()[SET_COOKIE]
        .to_str()
        .expect("cookie header is text")
        .split(';')
        .next()
        .expect("cookie has a name and value")
        .to_owned()
}

async fn identity(service: &Service, cookie: &str) -> String {
    let mut response = TestClient::get("http://localhost/me")
        .add_header(COOKIE, cookie, true)
        .send(service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    response
        .take_string()
        .await
        .expect("read identity response")
}

#[tokio::test]
async fn login_rotation_revokes_the_pre_authentication_cookie() {
    let service = service(true, None);
    let response = TestClient::get("http://localhost/me").send(&service).await;
    let old_cookie = session_cookie(&response);
    let response = TestClient::post("http://localhost/login")
        .add_header(COOKIE, &old_cookie, true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    let new_cookie = session_cookie(&response);
    assert_ne!(old_cookie, new_cookie);
    assert_eq!(identity(&service, &old_cookie).await, "anonymous");
    assert_eq!(identity(&service, &new_cookie).await, "victim");

    let response = TestClient::post("http://localhost/logout")
        .add_header(COOKIE, &new_cookie, true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(identity(&service, &old_cookie).await, "anonymous");
    assert_eq!(identity(&service, &new_cookie).await, "anonymous");
}

#[tokio::test]
async fn rotation_without_data_changes_is_saved_when_save_unchanged_is_disabled() {
    let service = service(false, None);
    let response = TestClient::post("http://localhost/login")
        .send(&service)
        .await;
    let old_cookie = session_cookie(&response);
    let response = TestClient::post("http://localhost/rotate")
        .add_header(COOKIE, &old_cookie, true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    let new_cookie = session_cookie(&response);
    assert_ne!(old_cookie, new_cookie);
    assert_eq!(identity(&service, &old_cookie).await, "anonymous");
    assert_eq!(identity(&service, &new_cookie).await, "victim");
}

#[tokio::test]
async fn an_in_flight_request_cannot_restore_a_rotated_or_logged_out_session() {
    for action in ["rotate", "logout"] {
        let entered = Arc::new(Notify::new());
        let resume = Arc::new(Notify::new());
        let service = service(
            true,
            Some(DelayedWrite {
                entered: entered.clone(),
                resume: resume.clone(),
            }),
        );
        let response = TestClient::post("http://localhost/login")
            .send(&service)
            .await;
        let old_cookie = session_cookie(&response);
        let late = TestClient::post("http://localhost/late")
            .add_header(COOKIE, &old_cookie, true)
            .send(&service);
        let revoke = async {
            entered.notified().await;
            let response = TestClient::post(format!("http://localhost/{action}"))
                .add_header(COOKIE, &old_cookie, true)
                .send(&service)
                .await;
            resume.notify_one();
            response
        };
        let (late, revoked) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::join!(late, revoke)
        })
        .await
        .expect("both requests should finish");
        assert_eq!(revoked.status_code, Some(StatusCode::OK));
        assert_eq!(late.status_code, Some(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!late.headers().contains_key(SET_COOKIE));
        assert_eq!(identity(&service, &old_cookie).await, "anonymous");
        if action == "rotate" {
            assert_eq!(
                identity(&service, &session_cookie(&revoked)).await,
                "victim"
            );
        }
    }
}
