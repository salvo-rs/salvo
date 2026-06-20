use salvo_core::{Depot, Request, Response};
use salvo_session::SessionDepotExt;

use super::{Flash, FlashHandler, FlashStore};

/// SessionStore is a `FlashStore` implementation that stores the flash messages in a session.
#[derive(Debug)]
#[non_exhaustive]
pub struct SessionStore {
    /// The session key for the flash messages.
    pub name: String,
}
impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore {
    /// Create a new `SessionStore`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            name: "salvo.flash".into(),
        }
    }

    /// Sets session key name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Converts into `FlashHandler`.
    #[must_use]
    pub fn into_handler(self) -> FlashHandler<Self> {
        FlashHandler::new(self)
    }
}

impl FlashStore for SessionStore {
    async fn load_flash(&self, _req: &mut Request, depot: &mut Depot) -> Option<Flash> {
        depot.session().and_then(|s| s.get::<Flash>(&self.name))
    }
    async fn save_flash(&self, _req: &mut Request, depot: &mut Depot, _res: &mut Response, flash: Flash) {
        let Some(session) = depot.session_mut() else {
            tracing::error!(
                "session is not available in depot; add SessionHandler before FlashHandler<SessionStore>"
            );
            return;
        };
        if let Err(e) = session.insert(&self.name, flash) {
            tracing::error!(error = ?e, "save flash to session failed");
        }
    }
    async fn clear_flash(&self, _req: &mut Request, depot: &mut Depot, _res: &mut Response) {
        let Some(session) = depot.session_mut() else {
            tracing::error!(
                "session is not available in depot; add SessionHandler before FlashHandler<SessionStore>"
            );
            return;
        };
        session.remove(&self.name);
    }
}
