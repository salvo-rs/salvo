//! Reader-side minimum levels must apply to messages written on another route.

#![cfg(feature = "cookie-store")]

use salvo_core::http::header::COOKIE;
use salvo_core::prelude::*;
use salvo_core::test::{ResponseExt, TestClient};
use salvo_flash::{CookieStore, FlashDepotExt, FlashHandler, FlashLevel};

#[handler]
async fn write_messages(depot: &mut Depot) -> &'static str {
    depot
        .outgoing_flash_mut()
        .debug("debug")
        .info("info")
        .warning("warning")
        .error("error");
    "written"
}

#[handler]
async fn read_messages(depot: &mut Depot) -> String {
    depot
        .incoming_flash()
        .map(|flash| {
            flash
                .iter()
                .map(|message| message.value.as_str())
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default()
}

#[handler]
async fn write_only_debug(depot: &mut Depot) -> &'static str {
    depot.outgoing_flash_mut().debug("debug");
    "written"
}

#[tokio::test]
async fn incoming_cookie_messages_obey_the_reader_minimum_and_are_consumed() {
    let key = salvo_core::http::cookie::Key::generate();
    let mut reader = FlashHandler::new(CookieStore::new().key(key.clone()));
    reader.minimum_level(FlashLevel::Warning);
    let service = Service::new(
        Router::new()
            .push(
                Router::with_path("write")
                    .hoop(CookieStore::new().key(key).into_handler())
                    .get(write_messages),
            )
            .push(Router::with_path("read").hoop(reader).get(read_messages)),
    );
    let response = TestClient::get("http://localhost/write")
        .send(&service)
        .await;
    let cookie = response.cookie("salvo.flash").unwrap().to_string();
    let mut response = TestClient::get("http://localhost/read")
        .add_header(COOKIE, cookie, true)
        .send(&service)
        .await;
    assert_eq!(response.take_string().await.unwrap(), "warning,error");
    assert_eq!(
        response
            .cookie("salvo.flash")
            .unwrap()
            .max_age()
            .unwrap()
            .whole_seconds(),
        0
    );
}

#[tokio::test]
async fn filtered_cookie_messages_are_cleared_and_unfiltered_readers_keep_all_levels() {
    let key = salvo_core::http::cookie::Key::generate();
    let mut reader = FlashHandler::new(CookieStore::new().key(key.clone()));
    reader.minimum_level(FlashLevel::Warning);
    let service = Service::new(
        Router::new()
            .push(
                Router::with_path("debug")
                    .hoop(CookieStore::new().key(key.clone()).into_handler())
                    .get(write_only_debug),
            )
            .push(
                Router::with_path("write")
                    .hoop(CookieStore::new().key(key.clone()).into_handler())
                    .get(write_messages),
            )
            .push(
                Router::with_path("all")
                    .hoop(CookieStore::new().key(key).into_handler())
                    .get(read_messages),
            )
            .push(Router::with_path("read").hoop(reader).get(read_messages)),
    );
    let response = TestClient::get("http://localhost/debug")
        .send(&service)
        .await;
    let cookie = response.cookie("salvo.flash").unwrap().to_string();
    let mut response = TestClient::get("http://localhost/read")
        .add_header(COOKIE, cookie, true)
        .send(&service)
        .await;
    assert_eq!(response.take_string().await.unwrap(), "");
    assert_eq!(
        response
            .cookie("salvo.flash")
            .unwrap()
            .max_age()
            .unwrap()
            .whole_seconds(),
        0
    );

    let response = TestClient::get("http://localhost/write")
        .send(&service)
        .await;
    let cookie = response.cookie("salvo.flash").unwrap().to_string();
    let mut response = TestClient::get("http://localhost/all")
        .add_header(COOKIE, cookie, true)
        .send(&service)
        .await;
    assert_eq!(
        response.take_string().await.unwrap(),
        "debug,info,warning,error"
    );
}

#[cfg(feature = "session-store")]
#[tokio::test]
async fn incoming_session_messages_obey_reader_policy_and_are_cleared() {
    use salvo_session::{MemoryStore, SessionHandler};

    let mut reader = FlashHandler::new(salvo_flash::SessionStore::new());
    reader.minimum_level(FlashLevel::Warning);
    let session = SessionHandler::builder(MemoryStore::new(), &[42; 64])
        .build()
        .unwrap();
    let service = Service::new(
        Router::new()
            .hoop(session)
            .push(
                Router::with_path("write")
                    .hoop(salvo_flash::SessionStore::new().into_handler())
                    .get(write_messages),
            )
            .push(
                Router::with_path("debug")
                    .hoop(salvo_flash::SessionStore::new().into_handler())
                    .get(write_only_debug),
            )
            .push(Router::with_path("read").hoop(reader).get(read_messages)),
    );
    for (path, expected) in [("write", "warning,error"), ("debug", "")] {
        let response = TestClient::get(format!("http://localhost/{path}"))
            .send(&service)
            .await;
        let cookie = response.cookie("salvo.session.id").unwrap().to_string();
        let mut response = TestClient::get("http://localhost/read")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert_eq!(response.take_string().await.unwrap(), expected);
        let cookie = response.cookie("salvo.session.id").unwrap().to_string();
        let mut response = TestClient::get("http://localhost/read")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert_eq!(response.take_string().await.unwrap(), "");
    }
}
