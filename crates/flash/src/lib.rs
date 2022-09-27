mod store;

pub use store::{CookieStore, FlashStore};

use std::fmt::{self, Formatter};

use salvo_core::{async_trait, Depot, FlowCtrl, Handler, Request, Response};
use serde::{Deserialize, Serialize};

// /// Helper function to create a new `FlashStore`.
// pub fn session_store() -> SessionStore {
//     FlashStore::new()
// }
/// Helper function to create a new `CookieStore`.
pub fn cookie_store() -> CookieStore {
    FlashStore::new()
}

/// Key for incoming flash messages in depot.
pub const INCOMING_FLASH_KEY: &str = "::salvo::extra::flash::incoming_flash";

/// Key for outgoing flash messages in depot.
pub const OUTGOING_FLASH_KEY: &str = "::salvo::extra::flash::outgoing_flash";

/// FlashDepotExt
pub trait FlashDepotExt {
    /// Push an `Debug` flash message.
    fn flash_debug(&mut self, message: impl Into<String>) -> &mut Self {
        self.flash_push(FlashLevel::Debug, message)
    }
    /// Push an `Info` flash message.
    fn flash_info(&mut self, message: impl Into<String>) -> &mut Self {
        self.flash_push(FlashLevel::Info, message)
    }
    /// Push an `Success` flash message.
    fn flash_success(&mut self, message: impl Into<String>) -> &mut Self {
        self.flash_push(FlashLevel::Success, message)
    }
    /// Push an `Warning` flash message.
    fn flash_warning(&mut self, message: impl Into<String>) -> &mut Self {
        self.flash_push(FlashLevel::Warning, message)
    }
    /// Push an `Error` flash message.
    fn flash_error(&mut self, message: impl Into<String>) -> &mut Self {
        self.flash_push(FlashLevel::Error, message)
    }
    /// Push a flash message with the given level and message.
    fn flash_push(&mut self, level: FlashLevel, message: impl Into<String>) -> &mut Self;

    /// Set incoming flash messages.
    fn set_incoming_flash(&mut self, messages: Vec<FlashMessage>) -> &mut Self;

    /// Take outgoing flash messages.
    fn take_outgoing_flash(&mut self) -> Option<Vec<FlashMessage>>;
}

impl FlashDepotExt for Depot {
    fn flash_push(&mut self, level: FlashLevel, message: impl Into<String>) -> &mut Self {
        self.get_mut::<Flash>(OUTGOING_FLASH_KEY).push(FlashMessage {
            level,
            value: message.into(),
        });
        self
    }
    /// Set incoming flash messages.
    fn set_incoming_flash(&mut self, messages: Vec<FlashMessage>) -> &mut Self {
        self.insert(INCOMING_FLASH_KEY, messages)
    }

    /// Take outgoing flash messages.
    fn take_outgoing_flash(&mut self) -> Option<Vec<FlashMessage>> {
        self.take(OUTGOING_FLASH_KEY)
    }
}

pub type Flash = Vec<FlashMessage>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashMessage {
    pub level: FlashLevel,
    pub value: String,
}

// Verbosity level of a flash message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FlashLevel {
    #[allow(missing_docs)]
    Debug = 0,
    #[allow(missing_docs)]
    Info = 1,
    #[allow(missing_docs)]
    Success = 2,
    #[allow(missing_docs)]
    Warning = 3,
    #[allow(missing_docs)]
    Error = 4,
}

/// FlashHandler
pub struct FlashHandler<S> {
    store: S,
}
impl<S: FlashStore> fmt::Debug for FlashHandler<S> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("FlashHandler").field("store", &self.store).finish()
    }
}
#[async_trait]
impl<S> Handler for FlashHandler<S>
where
    S: FlashStore,
{
    async fn handle(&self, req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
        depot.set_incoming_flash(self.store.load_flash().await);

        ctrl.call_next(req, depot, res).await;
        if ctrl.is_ceased() {
            return;
        }

        self.store.clear_flash(res);
        let flash = depot.take_outgoing_flash().unwrap_or_default();
        if !flash.is_empty() {
            self.store.save_flash(flash, res);
        }
    }
}

impl<S: FlashStore> FlashHandler<S> {
    /// Create new `FlashHandler`
    pub fn new(store: S) -> Self {
        Self { store }
    }
}

#[cfg(test)]
mod tests {
    use salvo_core::http::header::*;
    use salvo_core::http::Method;
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};
    use salvo_core::writer::Redirect;

    use super::*;

    #[test]
    fn test_session_data() {
        let handler = FlashHandler::builder(
            async_session::CookieStore,
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .cookie_domain("test.domain")
        .cookie_name("test_cookie")
        .cookie_path("/abc")
        .same_site_policy(SameSite::Strict)
        .session_ttl(Some(Duration::from_secs(30)))
        .build()
        .unwrap();
        assert_eq!(handler.cookie_domain, Some("test.domain".into()));
        assert_eq!(handler.cookie_name, "test_cookie");
        assert_eq!(handler.cookie_path, "/abc");
        assert_eq!(handler.same_site_policy, SameSite::Strict);
        assert_eq!(handler.session_ttl, Some(Duration::from_secs(30)));
    }

    #[tokio::test]
    async fn test_session_login() {
        #[handler]
        pub async fn login(req: &mut Request, depot: &mut Depot, res: &mut Response) {
            if req.method() == Method::POST {
                let mut session = Session::new();
                session
                    .insert("username", req.form::<String>("username").await.unwrap())
                    .unwrap();
                depot.set_session(session);
                res.render(Redirect::other("/").unwrap());
            } else {
                res.render(Text::Html("login page"));
            }
        }

        #[handler]
        pub async fn logout(depot: &mut Depot, res: &mut Response) {
            if let Some(session) = depot.session_mut() {
                session.remove("username");
            }
            res.render(Redirect::other("/").unwrap());
        }

        #[handler]
        pub async fn home(depot: &mut Depot, res: &mut Response) {
            let mut content = r#"home"#.into();
            if let Some(session) = depot.session_mut() {
                if let Some(username) = session.get::<String>("username") {
                    content = username;
                }
            }
            res.render(Text::Html(content));
        }

        let session_handler = FlashHandler::builder(
            MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .build()
        .unwrap();
        let router = Router::new()
            .hoop(session_handler)
            .get(home)
            .push(Router::with_path("login").get(login).post(login))
            .push(Router::with_path("logout").get(logout));
        let service = Service::new(router);

        let respone = TestClient::post("http://127.0.0.1:7878/login")
            .raw_form("username=salvo")
            .send(&service)
            .await;
        assert_eq!(respone.status_code(), Some(StatusCode::SEE_OTHER));
        let cookie = respone.headers().get(SET_COOKIE).unwrap();

        let mut respone = TestClient::get("http://127.0.0.1:7878/")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert_eq!(respone.take_string().await.unwrap(), "salvo");

        let respone = TestClient::get("http://127.0.0.1:7878/logout").send(&service).await;
        assert_eq!(respone.status_code(), Some(StatusCode::SEE_OTHER));

        let mut respone = TestClient::get("http://127.0.0.1:7878/").send(&service).await;
        assert_eq!(respone.take_string().await.unwrap(), "home");
    }
}
