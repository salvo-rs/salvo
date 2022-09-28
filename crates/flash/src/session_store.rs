use salvo_core::{async_trait, Depot, Request, Response, Result};
use salvo_session::SessionDepotExt;

use super::{Flash, FlashHandler, FlashStore};

#[derive(Debug)]
pub struct SessionStore {
    pub name: String,
}
impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Into<FlashHandler<SessionStore>> for SessionStore {
    fn into(self) -> FlashHandler<SessionStore> {
        FlashHandler::new(self)
    }
}

impl SessionStore {
    /// Create a new `SessionStore`.
    pub fn new() -> Self {
        Self { name: "_flash".into() }
    }

    /// Set cookie name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
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
    async fn save_flash(&self, flash: Flash, depot: &mut Depot, _res: &mut Response) {
        if let Err(e) = depot
            .session_mut()
            .expect("session must be exist")
            .insert(&self.name, &serde_json::to_string(&flash).unwrap_or_default())
        {
            tracing::error!(error = ?e, "save flash to session failed");
        }
    }
    async fn clear_flash(&self, depot: &mut Depot, _res: &mut Response) {
        depot.session_mut().expect("session must be exist").remove(&self.name);
    }
}
