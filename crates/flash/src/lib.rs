//! The flash message lib for Salvo web framework.
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::fmt::{self, Debug, Display, Formatter};
use std::ops::Deref;

use salvo_core::{Depot, FlowCtrl, Handler, Request, Response, async_trait};
use serde::{Deserialize, Serialize};

#[macro_use]
mod cfg;

cfg_feature! {
    #![feature = "cookie-store"]

    mod cookie_store;
    pub use cookie_store::CookieStore;

    /// Helper function to create a `CookieStore`.
    pub fn cookie_store() -> CookieStore {
        CookieStore::new()
    }
}

cfg_feature! {
    #![feature = "session-store"]

    mod session_store;
    pub use session_store::SessionStore;

    /// Helper function to create a `SessionStore`.
    pub fn session_store() -> SessionStore {
        SessionStore::new()
    }
}

/// Key for incoming flash messages in depot.
pub const INCOMING_FLASH_KEY: &str = "::salvo::flash::incoming_flash";

/// Key for outgoing flash messages in depot.
pub const OUTGOING_FLASH_KEY: &str = "::salvo::flash::outgoing_flash";

/// A flash is a list of messages.
#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct Flash(pub Vec<FlashMessage>);
impl Flash {
    /// Add a new message with level `Debug`.
    #[inline]
    pub fn debug(&mut self, message: impl Into<String>) -> &mut Self {
        self.0.push(FlashMessage::debug(message));
        self
    }
    /// Add a new message with level `Info`.
    #[inline]
    pub fn info(&mut self, message: impl Into<String>) -> &mut Self {
        self.0.push(FlashMessage::info(message));
        self
    }
    /// Add a new message with level `Success`.
    #[inline]
    pub fn success(&mut self, message: impl Into<String>) -> &mut Self {
        self.0.push(FlashMessage::success(message));
        self
    }
    /// Add a new message with level `Waring`.
    #[inline]
    pub fn warning(&mut self, message: impl Into<String>) -> &mut Self {
        self.0.push(FlashMessage::warning(message));
        self
    }
    /// Add a new message with level `Error`.
    #[inline]
    pub fn error(&mut self, message: impl Into<String>) -> &mut Self {
        self.0.push(FlashMessage::error(message));
        self
    }
}

impl Deref for Flash {
    type Target = Vec<FlashMessage>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A flash message.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[non_exhaustive]
pub struct FlashMessage {
    /// Flash message level.
    pub level: FlashLevel,
    /// Flash message content.
    pub value: String,
}
impl FlashMessage {
    /// Create a new `FlashMessage` with `FlashLevel::Debug`.
    #[inline]
    pub fn debug(message: impl Into<String>) -> Self {
        Self {
            level: FlashLevel::Debug,
            value: message.into(),
        }
    }
    /// Create a new `FlashMessage` with `FlashLevel::Info`.
    #[inline]
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: FlashLevel::Info,
            value: message.into(),
        }
    }
    /// Create a new `FlashMessage` with `FlashLevel::Success`.
    #[inline]
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            level: FlashLevel::Success,
            value: message.into(),
        }
    }
    /// Create a new `FlashMessage` with `FlashLevel::Warning`.
    #[inline]
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            level: FlashLevel::Warning,
            value: message.into(),
        }
    }
    /// create a new `FlashMessage` with `FlashLevel::Error`.
    #[inline]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            level: FlashLevel::Error,
            value: message.into(),
        }
    }
}

/// Verbosity level of a flash message.
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
    /// Convert a `FlashLevel` to a `&str`.
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

/// `FlashStore` is for stores flash messages.
pub trait FlashStore: Debug + Send + Sync + 'static {
    /// Get the flash messages from the store.
    fn load_flash(
        &self,
        req: &mut Request,
        depot: &mut Depot,
    ) -> impl Future<Output = Option<Flash>> + Send;
    /// Save the flash messages to the store.
    fn save_flash(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        flash: Flash,
    ) -> impl Future<Output = ()> + Send;
    /// Clear the flash store.
    fn clear_flash(&self, depot: &mut Depot, res: &mut Response)
    -> impl Future<Output = ()> + Send;
}

/// A trait for `Depot` to get flash messages.
pub trait FlashDepotExt {
    /// Get incoming flash.
    fn incoming_flash(&mut self) -> Option<&Flash>;
    /// Get outgoing flash.
    fn outgoing_flash(&self) -> &Flash;
    /// Get mutable outgoing flash.
    fn outgoing_flash_mut(&mut self) -> &mut Flash;
}

impl FlashDepotExt for Depot {
    #[inline]
    fn incoming_flash(&mut self) -> Option<&Flash> {
        self.get::<Flash>(INCOMING_FLASH_KEY).ok()
    }

    #[inline]
    fn outgoing_flash(&self) -> &Flash {
        self.get::<Flash>(OUTGOING_FLASH_KEY)
            .expect("Flash should be initialized")
    }

    #[inline]
    fn outgoing_flash_mut(&mut self) -> &mut Flash {
        self.get_mut::<Flash>(OUTGOING_FLASH_KEY)
            .expect("Flash should be initialized")
    }
}

/// `FlashHandler` is a middleware for flash messages.
#[non_exhaustive]
pub struct FlashHandler<S> {
    store: S,
    /// Minimum level of messages to be displayed.
    pub minimum_level: Option<FlashLevel>,
}
impl<S> FlashHandler<S> {
    /// Create a new `FlashHandler` with the given `FlashStore`.
    #[inline]
    pub fn new(store: S) -> Self {
        Self {
            store,
            minimum_level: None,
        }
    }

    /// Sets the minimum level of messages to be displayed.
    #[inline]
    pub fn minimum_level(&mut self, level: impl Into<Option<FlashLevel>>) -> &mut Self {
        self.minimum_level = level.into();
        self
    }
}
impl<S: FlashStore> fmt::Debug for FlashHandler<S> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("FlashHandler")
            .field("store", &self.store)
            .finish()
    }
}
#[async_trait]
impl<S> Handler for FlashHandler<S>
where
    S: FlashStore,
{
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
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

        let mut flash = depot
            .remove::<Flash>(OUTGOING_FLASH_KEY)
            .unwrap_or_default();
        if let Some(min_level) = self.minimum_level {
            flash.0.retain(|msg| msg.level >= min_level);
        }
        if !flash.is_empty() {
            self.store.save_flash(req, depot, res, flash).await;
        } else if has_incoming {
            self.store.clear_flash(depot, res).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use salvo_core::http::header::{COOKIE, SET_COOKIE};
    use salvo_core::prelude::*;
    use salvo_core::test::{ResponseExt, TestClient};

    use super::*;

    #[handler]
    pub async fn set_flash(depot: &mut Depot, res: &mut Response) {
        let flash = depot.outgoing_flash_mut();
        flash.info("Hey there!").debug("How is it going?");
        res.render(Redirect::other("/get"));
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

    #[cfg(feature = "cookie-store")]
    #[tokio::test]
    async fn test_cookie_store() {
        let cookie_name = "my-custom-cookie-name".to_string();
        let router = Router::new()
            .hoop(CookieStore::new().name(&cookie_name).into_handler())
            .push(Router::with_path("get").get(get_flash))
            .push(Router::with_path("set").get(set_flash));
        let service = Service::new(router);

        let respone = TestClient::get("http://127.0.0.1:5800/set")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::SEE_OTHER));

        let cookie = respone.headers().get(SET_COOKIE).unwrap();
        assert!(cookie.to_str().unwrap().contains(&cookie_name));

        let mut respone = TestClient::get("http://127.0.0.1:5800/get")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert!(respone.take_string().await.unwrap().contains("Hey there!"));

        let cookie = respone.headers().get(SET_COOKIE).unwrap();
        assert!(cookie.to_str().unwrap().contains(&cookie_name));

        let mut respone = TestClient::get("http://127.0.0.1:5800/get")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert!(respone.take_string().await.unwrap().is_empty());
    }

    #[cfg(feature = "session-store")]
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
            .hoop(SessionStore::new().name(&session_name).into_handler())
            .push(Router::with_path("get").get(get_flash))
            .push(Router::with_path("set").get(set_flash));
        let service = Service::new(router);

        let respone = TestClient::get("http://127.0.0.1:5800/set")
            .send(&service)
            .await;
        assert_eq!(respone.status_code, Some(StatusCode::SEE_OTHER));

        let cookie = respone.headers().get(SET_COOKIE).unwrap();

        let mut respone = TestClient::get("http://127.0.0.1:5800/get")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert!(respone.take_string().await.unwrap().contains("Hey there!"));

        let mut respone = TestClient::get("http://127.0.0.1:5800/get")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert!(respone.take_string().await.unwrap().is_empty());
    }
}
