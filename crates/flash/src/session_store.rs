use salvo_core::{async_trait, Depot, Request, Response};
use salvo_session::SessionDepotExt;

use super::{Flash, FlashHandler, FlashStore};

/// SessionStore is a `FlashStore` implementation that stores the flash messages in a session.
#[derive(Debug)]
pub struct SessionStore {
    /// The cookie name for the flash messages.
    pub name: String,
}
impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore {
    /// Create a new `SessionStore`.
    pub fn new() -> Self {
        Self {
            name: "salvo.flash".into(),
        }
    }

    /// Sets cookie name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Into `FlashHandler`.
    pub fn into_handler(self) -> FlashHandler<SessionStore> {
        FlashHandler::new(self)
    }
}
#[async_trait]
impl FlashStore for SessionStore {
    async fn load_flash(&self, _req: &mut Request, depot: &mut Depot) -> Option<Flash> {
        depot.session().and_then(|s| s.get::<Flash>(&self.name))
    }
    async fn save_flash(&self, _req: &mut Request, depot: &mut Depot, _res: &mut Response, flash: Flash) {
        if let Err(e) = depot
            .session_mut()
            .expect("session must be exist")
            .insert(&self.name, flash)
        {
            tracing::error!(error = ?e, "save flash to session failed");
        }
    }
    async fn clear_flash(&self, depot: &mut Depot, _res: &mut Response) {
        depot.session_mut().expect("session must be exist").remove(&self.name);
    }
}
