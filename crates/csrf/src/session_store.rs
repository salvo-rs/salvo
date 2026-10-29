use salvo_core::{Depot, Error, Request, Response};
use salvo_session::SessionDepotExt;

use super::{CsrfCipher, CsrfStore};

/// A `CsrfStore` implementation that stores the CSRF proof in a session.
///
/// Stored proofs are checked with the current cipher before reuse. An invalid
/// proof is treated as missing, allowing a safe request to issue a fresh token
/// after a key or token-format change; unsafe requests still require a valid pair.
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
    async fn load<C: CsrfCipher>(&self, _req: &mut Request, depot: &mut Depot, cipher: &C) -> Option<(String, String)> {
        depot
            .session()
            .and_then(|s| s.get::<String>(&self.name))
            .and_then(|s| s.split_once('.').map(|(t, p)| (t.to_owned(), p.to_owned())))
            .filter(|(token, proof)| cipher.verify(token, proof))
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
