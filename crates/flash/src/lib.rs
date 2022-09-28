use std::fmt::{self, Debug, Display, Formatter};
use std::ops::Deref;

use salvo_core::{async_trait, Depot, FlowCtrl, Handler, Request, Response};
use serde::{Deserialize, Serialize};

#[macro_use]
mod cfg;

cfg_feature! {
    #![feature = "cookie_store"]

    mod cookie_store;
    pub use cookie_store::CookieStore;
}

cfg_feature! {
    #![feature = "session_store"]

    mod session_store;
    pub use session_store::SessionStore;
}

/// Key for incoming flash messages in depot.
pub const INCOMING_FLASH_KEY: &str = "::salvo_flash::incoming_flash";

/// Key for outgoing flash messages in depot.
pub const OUTGOING_FLASH_KEY: &str = "::salvo_flash::outgoing_flash";

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct Flash(pub Vec<FlashMessage>);
impl Flash {
    /// Add a new message with level `Debug`.
    pub fn debug(&mut self, message: impl Into<String>) -> &mut Self {
        self.0.push(FlashMessage::debug(message));
        self
    }
    /// Add a new message with level `Info`.
    pub fn info(&mut self, message: impl Into<String>) -> &mut Self {
        self.0.push(FlashMessage::info(message));
        self
    }
    /// Add a new message with level `Success`.
    pub fn success(&mut self, message: impl Into<String>) -> &mut Self {
        self.0.push(FlashMessage::success(message));
        self
    }
    /// Add a new message with level `Waring`.
    pub fn warning(&mut self, message: impl Into<String>) -> &mut Self {
        self.0.push(FlashMessage::warning(message));
        self
    }
    /// Add a new message with level `Error`.
    pub fn error(&mut self, message: impl Into<String>) -> &mut Self {
        self.0.push(FlashMessage::warning(message));
        self
    }
}

impl Deref for Flash {
    type Target = Vec<FlashMessage>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FlashMessage {
    pub level: FlashLevel,
    pub value: String,
}
impl FlashMessage {
    /// Create a new `FlashMessage` with `FlashLevel::Debug`.
    pub fn debug(message: impl Into<String>) -> Self {
        Self {
            level: FlashLevel::Debug,
            value: message.into(),
        }
    }
    /// Create a new `FlashMessage` with `FlashLevel::Info`.
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: FlashLevel::Info,
            value: message.into(),
        }
    }
    /// Create a new `FlashMessage` with `FlashLevel::Success`.
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            level: FlashLevel::Success,
            value: message.into(),
        }
    }
    /// Create a new `FlashMessage` with `FlashLevel::Warning`.
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            level: FlashLevel::Warning,
            value: message.into(),
        }
    }
    /// create a new `FlashMessage` with `FlashLevel::Error`.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            level: FlashLevel::Error,
            value: message.into(),
        }
    }
}

// Verbosity level of a flash message.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
impl FlashLevel {
    pub fn to_str(&self) -> &'static str {
        match self {
            FlashLevel::Debug => "debug",
            FlashLevel::Info => "info",
            FlashLevel::Success => "success",
            FlashLevel::Warning => "warning",
            FlashLevel::Error => "error",
        }
    }
}
impl Debug for FlashLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl Display for FlashLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

#[async_trait]
pub trait FlashStore: Debug + Send + Sync + 'static {
    async fn load_flash(&self, req: &mut Request, depot: &mut Depot) -> Option<Flash>;
    async fn save_flash(&self, flash: Flash, depot: &mut Depot, res: &mut Response);
    async fn clear_flash(&self, depot: &mut Depot, res: &mut Response);
}

/// FlashDepotExt
pub trait FlashDepotExt {
    /// Get incoming flash.
    fn incoming_flash(&mut self) -> Option<&Flash>;

    /// Get outgoing flash.
    fn outgoing_flash(&mut self) -> &Flash;
    /// Get mutable outgoing flash.
    fn outgoing_flash_mut(&mut self) -> &mut Flash;
}

impl FlashDepotExt for Depot {
    fn incoming_flash(&mut self) -> Option<&Flash> {
        self.get::<Flash>(INCOMING_FLASH_KEY)
    }

    fn outgoing_flash(&mut self) -> &Flash {
        self.get::<Flash>(OUTGOING_FLASH_KEY)
            .expect("Flash should be initialized")
    }

    fn outgoing_flash_mut(&mut self) -> &mut Flash {
        self.get_mut::<Flash>(OUTGOING_FLASH_KEY)
            .expect("Flash should be initialized")
    }
}

/// FlashHandler
pub struct FlashHandler<S> {
    store: S,
    pub minimum_level: Option<FlashLevel>,
}
impl<S> FlashHandler<S> {
    /// Create a new `FlashHandler` with the given `FlashStore`.
    pub fn new(store: S) -> Self {
        Self {
            store,
            minimum_level: None,
        }
    }

    /// Set the minimum level of messages to be displayed.
    pub fn minimum_level(&mut self, level: impl Into<Option<FlashLevel>>) -> &mut Self {
        self.minimum_level = level.into();
        self
    }
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
        let mut has_incoming = false;
        if let Some(flash) = self.store.load_flash(req, depot).await {
            has_incoming = !flash.is_empty();
            depot.insert(INCOMING_FLASH_KEY, flash);
        }
        depot.insert(OUTGOING_FLASH_KEY, Flash(vec![]));

        ctrl.call_next(req, depot, res).await;
        if ctrl.is_ceased() {
            return;
        }

        let mut flash = depot.remove::<Flash>(OUTGOING_FLASH_KEY).unwrap_or_default();
        if let Some(min_level) = self.minimum_level {
            flash.0.retain(|msg| msg.level >= min_level);
        }
        if !flash.is_empty() {
            self.store.save_flash(flash, depot, res).await;
        } else if has_incoming {
            self.store.clear_flash(depot, res).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use salvo_core::http::header::*;
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};
    use salvo_core::writer::Redirect;

    use super::*;

    #[handler]
    pub async fn set_flash(depot: &mut Depot, res: &mut Response) {
        let flash = depot.outgoing_flash_mut();
        flash.info("Hey there!").debug("How is it going?");
        res.render(Redirect::other("/get").unwrap());
    }

    #[handler]
    pub async fn get_flash(depot: &mut Depot, _res: &mut Response) -> String {
        let mut body = String::new();
        if let Some(flash) = depot.incoming_flash() {
            for message in flash.iter() {
                writeln!(body, "{} - {}", message.value, message.level).unwrap();
            }
        }
        body
    }

    #[cfg(feature = "cookie_store")]
    #[tokio::test]
    async fn test_cookie_store() {
        let cookie_name = "my-custom-cookie-name".to_string();
        let router = Router::new()
            .hoop(CookieStore::new().with_name(&cookie_name).into_handler())
            .push(Router::with_path("get").get(get_flash))
            .push(Router::with_path("set").get(set_flash));
        let service = Service::new(router);

        let respone = TestClient::get("http://127.0.0.1:7878/set").send(&service).await;
        assert_eq!(respone.status_code(), Some(StatusCode::SEE_OTHER));

        let cookie = respone.headers().get(SET_COOKIE).unwrap();
        assert!(cookie.to_str().unwrap().contains(&cookie_name));

        let mut respone = TestClient::get("http://127.0.0.1:7878/get")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert!(respone.take_string().await.unwrap().contains("Hey there!"));

        let cookie = respone.headers().get(SET_COOKIE).unwrap();
        assert!(cookie.to_str().unwrap().contains(&cookie_name));

        let mut respone = TestClient::get("http://127.0.0.1:7878/get")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert!(respone.take_string().await.unwrap().is_empty());
    }

    #[cfg(feature = "session_store")]
    #[tokio::test]
    async fn test_session_store() {
        let session_handler = salvo_session::SessionHandler::builder(
            salvo_session::MemoryStore::new(),
            b"secretabsecretabsecretabsecretabsecretabsecretabsecretabsecretab",
        )
        .build()
        .unwrap();

        let session_name = "my-custom-session-name".to_string();
        let router = Router::new()
            .hoop(session_handler)
            .hoop(SessionStore::new().with_name(&session_name).into_handler())
            .push(Router::with_path("get").get(get_flash))
            .push(Router::with_path("set").get(set_flash));
        let service = Service::new(router);

        let respone = TestClient::get("http://127.0.0.1:7878/set").send(&service).await;
        assert_eq!(respone.status_code(), Some(StatusCode::SEE_OTHER));

        let cookie = respone.headers().get(SET_COOKIE).unwrap();

        let mut respone = TestClient::get("http://127.0.0.1:7878/get")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert!(respone.take_string().await.unwrap().contains("Hey there!"));

        let mut respone = TestClient::get("http://127.0.0.1:7878/get")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert!(respone.take_string().await.unwrap().is_empty());
    }
}
