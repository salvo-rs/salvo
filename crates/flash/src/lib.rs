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
    #[must_use] pub fn cookie_store() -> CookieStore {
        CookieStore::new()
    }
}

cfg_feature! {
    #![feature = "session-store"]

    mod session_store;
    pub use session_store::SessionStore;

    /// Helper function to create a `SessionStore`.
    #[must_use]
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
    /// Add a new message with level `Warning`.
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
    #[must_use]
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
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
        let cookie_name = "my-custom-cookie-name".to_owned();
        let router = Router::new()
            .hoop(CookieStore::new().name(&cookie_name).into_handler())
            .push(Router::with_path("get").get(get_flash))
            .push(Router::with_path("set").get(set_flash));
        let service = Service::new(router);

        let response = TestClient::get("http://127.0.0.1:8698/set")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::SEE_OTHER));

        let cookie = response.headers().get(SET_COOKIE).unwrap();
        assert!(cookie.to_str().unwrap().contains(&cookie_name));

        let mut response = TestClient::get("http://127.0.0.1:8698/get")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert!(response.take_string().await.unwrap().contains("Hey there!"));

        let cookie = response.headers().get(SET_COOKIE).unwrap();
        assert!(cookie.to_str().unwrap().contains(&cookie_name));

        let mut response = TestClient::get("http://127.0.0.1:8698/get")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert!(response.take_string().await.unwrap().is_empty());
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

        let response = TestClient::get("http://127.0.0.1:8698/set")
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::SEE_OTHER));

        let cookie = response.headers().get(SET_COOKIE).unwrap();

        let mut response = TestClient::get("http://127.0.0.1:8698/get")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert!(response.take_string().await.unwrap().contains("Hey there!"));

        let mut response = TestClient::get("http://127.0.0.1:8698/get")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;
        assert!(response.take_string().await.unwrap().is_empty());
    }

    // Tests for Flash struct
    #[test]
    fn test_flash_default() {
        let flash = Flash::default();
        assert!(flash.0.is_empty());
    }

    #[test]
    fn test_flash_debug() {
        let mut flash = Flash::default();
        flash.info("test message");
        let debug_str = format!("{:?}", flash);
        assert!(debug_str.contains("Flash"));
        assert!(debug_str.contains("test message"));
    }

    #[test]
    fn test_flash_clone() {
        let mut flash = Flash::default();
        flash.info("test");
        let cloned = flash.clone();
        assert_eq!(flash.0.len(), cloned.0.len());
    }

    #[test]
    fn test_flash_add_debug() {
        let mut flash = Flash::default();
        flash.debug("debug message");
        assert_eq!(flash.0.len(), 1);
        assert_eq!(flash.0[0].level, FlashLevel::Debug);
        assert_eq!(flash.0[0].value, "debug message");
    }

    #[test]
    fn test_flash_add_info() {
        let mut flash = Flash::default();
        flash.info("info message");
        assert_eq!(flash.0.len(), 1);
        assert_eq!(flash.0[0].level, FlashLevel::Info);
        assert_eq!(flash.0[0].value, "info message");
    }

    #[test]
    fn test_flash_add_success() {
        let mut flash = Flash::default();
        flash.success("success message");
        assert_eq!(flash.0.len(), 1);
        assert_eq!(flash.0[0].level, FlashLevel::Success);
        assert_eq!(flash.0[0].value, "success message");
    }

    #[test]
    fn test_flash_add_warning() {
        let mut flash = Flash::default();
        flash.warning("warning message");
        assert_eq!(flash.0.len(), 1);
        assert_eq!(flash.0[0].level, FlashLevel::Warning);
        assert_eq!(flash.0[0].value, "warning message");
    }

    #[test]
    fn test_flash_add_error() {
        let mut flash = Flash::default();
        flash.error("error message");
        assert_eq!(flash.0.len(), 1);
        assert_eq!(flash.0[0].level, FlashLevel::Error);
        assert_eq!(flash.0[0].value, "error message");
    }

    #[test]
    fn test_flash_chain_messages() {
        let mut flash = Flash::default();
        flash
            .debug("debug")
            .info("info")
            .success("success")
            .warning("warning")
            .error("error");
        assert_eq!(flash.0.len(), 5);
    }

    #[test]
    fn test_flash_deref() {
        let mut flash = Flash::default();
        flash.info("test");
        // Deref to Vec<FlashMessage>
        assert_eq!(flash.len(), 1);
        assert!(flash.iter().any(|m| m.value == "test"));
    }

    // Tests for FlashMessage
    #[test]
    fn test_flash_message_debug() {
        let msg = FlashMessage::debug("debug msg");
        assert_eq!(msg.level, FlashLevel::Debug);
        assert_eq!(msg.value, "debug msg");
    }

    #[test]
    fn test_flash_message_info() {
        let msg = FlashMessage::info("info msg");
        assert_eq!(msg.level, FlashLevel::Info);
        assert_eq!(msg.value, "info msg");
    }

    #[test]
    fn test_flash_message_success() {
        let msg = FlashMessage::success("success msg");
        assert_eq!(msg.level, FlashLevel::Success);
        assert_eq!(msg.value, "success msg");
    }

    #[test]
    fn test_flash_message_warning() {
        let msg = FlashMessage::warning("warning msg");
        assert_eq!(msg.level, FlashLevel::Warning);
        assert_eq!(msg.value, "warning msg");
    }

    #[test]
    fn test_flash_message_error() {
        let msg = FlashMessage::error("error msg");
        assert_eq!(msg.level, FlashLevel::Error);
        assert_eq!(msg.value, "error msg");
    }

    #[test]
    fn test_flash_message_clone() {
        let msg = FlashMessage::info("test");
        let cloned = msg.clone();
        assert_eq!(msg.level, cloned.level);
        assert_eq!(msg.value, cloned.value);
    }

    #[test]
    fn test_flash_message_debug_trait() {
        let msg = FlashMessage::info("test");
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("FlashMessage"));
        assert!(debug_str.contains("test"));
    }

    // Tests for FlashLevel
    #[test]
    fn test_flash_level_to_str() {
        assert_eq!(FlashLevel::Debug.to_str(), "debug");
        assert_eq!(FlashLevel::Info.to_str(), "info");
        assert_eq!(FlashLevel::Success.to_str(), "success");
        assert_eq!(FlashLevel::Warning.to_str(), "warning");
        assert_eq!(FlashLevel::Error.to_str(), "error");
    }

    #[test]
    fn test_flash_level_debug_trait() {
        assert_eq!(format!("{:?}", FlashLevel::Debug), "debug");
        assert_eq!(format!("{:?}", FlashLevel::Info), "info");
        assert_eq!(format!("{:?}", FlashLevel::Success), "success");
        assert_eq!(format!("{:?}", FlashLevel::Warning), "warning");
        assert_eq!(format!("{:?}", FlashLevel::Error), "error");
    }

    #[test]
    fn test_flash_level_display() {
        assert_eq!(format!("{}", FlashLevel::Debug), "debug");
        assert_eq!(format!("{}", FlashLevel::Info), "info");
        assert_eq!(format!("{}", FlashLevel::Success), "success");
        assert_eq!(format!("{}", FlashLevel::Warning), "warning");
        assert_eq!(format!("{}", FlashLevel::Error), "error");
    }

    #[test]
    fn test_flash_level_ord() {
        assert!(FlashLevel::Debug < FlashLevel::Info);
        assert!(FlashLevel::Info < FlashLevel::Success);
        assert!(FlashLevel::Success < FlashLevel::Warning);
        assert!(FlashLevel::Warning < FlashLevel::Error);
    }

    #[test]
    fn test_flash_level_eq() {
        assert_eq!(FlashLevel::Debug, FlashLevel::Debug);
        assert_ne!(FlashLevel::Debug, FlashLevel::Info);
    }

    #[test]
    fn test_flash_level_clone() {
        let level = FlashLevel::Info;
        let cloned = level;
        assert_eq!(level, cloned);
    }

    #[test]
    fn test_flash_level_copy() {
        let level = FlashLevel::Warning;
        let copied = level;
        assert_eq!(level, copied);
    }

    // Tests for FlashHandler
    #[test]
    fn test_flash_handler_new() {
        #[cfg(feature = "cookie-store")]
        {
            let handler = FlashHandler::new(CookieStore::new());
            assert!(handler.minimum_level.is_none());
        }
    }

    #[test]
    fn test_flash_handler_minimum_level() {
        #[cfg(feature = "cookie-store")]
        {
            let mut handler = FlashHandler::new(CookieStore::new());
            handler.minimum_level(FlashLevel::Warning);
            assert_eq!(handler.minimum_level, Some(FlashLevel::Warning));
        }
    }

    #[test]
    fn test_flash_handler_minimum_level_none() {
        #[cfg(feature = "cookie-store")]
        {
            let mut handler = FlashHandler::new(CookieStore::new());
            handler.minimum_level(FlashLevel::Info);
            handler.minimum_level(None);
            assert!(handler.minimum_level.is_none());
        }
    }

    #[test]
    fn test_flash_handler_debug() {
        #[cfg(feature = "cookie-store")]
        {
            let handler = FlashHandler::new(CookieStore::new());
            let debug_str = format!("{:?}", handler);
            assert!(debug_str.contains("FlashHandler"));
            assert!(debug_str.contains("store"));
        }
    }

    // Tests for Flash serialization
    #[test]
    fn test_flash_serialization() {
        let mut flash = Flash::default();
        flash.info("test message");

        let serialized = serde_json::to_string(&flash).unwrap();
        let deserialized: Flash = serde_json::from_str(&serialized).unwrap();

        assert_eq!(flash.0.len(), deserialized.0.len());
        assert_eq!(flash.0[0].value, deserialized.0[0].value);
        assert_eq!(flash.0[0].level, deserialized.0[0].level);
    }

    #[test]
    fn test_flash_message_serialization() {
        let msg = FlashMessage::warning("test");

        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: FlashMessage = serde_json::from_str(&serialized).unwrap();

        assert_eq!(msg.value, deserialized.value);
        assert_eq!(msg.level, deserialized.level);
    }

    #[test]
    fn test_flash_level_serialization() {
        let level = FlashLevel::Error;

        let serialized = serde_json::to_string(&level).unwrap();
        let deserialized: FlashLevel = serde_json::from_str(&serialized).unwrap();

        assert_eq!(level, deserialized);
    }

    #[cfg(feature = "cookie-store")]
    #[tokio::test]
    async fn test_flash_handler_filters_by_minimum_level() {
        #[handler]
        pub async fn set_all_levels(depot: &mut Depot, res: &mut Response) {
            let flash = depot.outgoing_flash_mut();
            flash
                .debug("debug msg")
                .info("info msg")
                .warning("warning msg")
                .error("error msg");
            res.render(Redirect::other("/get"));
        }

        let mut handler = FlashHandler::new(CookieStore::new());
        handler.minimum_level(FlashLevel::Warning);

        let router = Router::new()
            .hoop(handler)
            .push(Router::with_path("get").get(get_flash))
            .push(Router::with_path("set").get(set_all_levels));
        let service = Service::new(router);

        let response = TestClient::get("http://127.0.0.1:8698/set")
            .send(&service)
            .await;

        let cookie = response.headers().get(SET_COOKIE).unwrap();

        let mut response = TestClient::get("http://127.0.0.1:8698/get")
            .add_header(COOKIE, cookie, true)
            .send(&service)
            .await;

        let body = response.take_string().await.unwrap();

        // Should contain warning and error, but not debug and info
        assert!(body.contains("warning msg"));
        assert!(body.contains("error msg"));
        assert!(!body.contains("debug msg"));
        assert!(!body.contains("info msg"));
    }
}
