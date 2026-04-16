use salvo_core::{Depot, Error, Request, Response};
use salvo_session::SessionDepotExt;

use super::{CsrfCipher, CsrfStore};

/// A `CsrfStore` implementation that stores the CSRF proof in a session.
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            name: "salvo.csrf".into(),
        }
    }
}

impl CsrfStore for SessionStore {
    type Error = Error;
    async fn load<C: CsrfCipher>(&self, _req: &mut Request, depot: &mut Depot, _cipher: &C) -> Option<(String, String)> {
        depot
            .session()
            .and_then(|s| s.get::<String>(&self.name))
            .and_then(|s| s.split_once('.').map(|(t, p)| (t.into(), p.into())))
    }
    async fn save(
        &self,
        _req: &mut Request,
        depot: &mut Depot,
        _res: &mut Response,
        token: &str,
        proof: &str,
    ) -> Result<(), Self::Error> {
        let Some(session) = depot.session_mut() else {
            return Err(Error::other(
                "session is not available in depot; add SessionHandler before Csrf<_, SessionStore>",
            ));
        };
        session.insert(&self.name, format!("{token}.{proof}"))?;
        Ok(())
    }
}
