use salvo_core::{async_trait, Depot, Error, Request, Response};
use salvo_session::SessionDepotExt;

use super::CsrfStore;

/// CookieStore is a `CsrfStore` implementation that stores the CSRF secret in a session.
#[derive(Debug)]
pub struct SessionStore {
    name: String,
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
            name: "salvo.csrf.secret".into(),
        }
    }
}

#[async_trait]
impl CsrfStore for SessionStore {
    type Error = Error;
    async fn load_secret(&self, _req: &mut Request, depot: &mut Depot) -> Option<Vec<u8>> {
        depot.session().and_then(|s| s.get::<Vec<u8>>(&self.name))
    }
    async fn save_secret(
        &self,
        _req: &mut Request,
        depot: &mut Depot,
        _res: &mut Response,
        secret: &[u8],
    ) -> Result<(), Self::Error> {
        depot
            .session_mut()
            .expect("session must be exist")
            .insert(&self.name, secret)?;
        Ok(())
    }
}
