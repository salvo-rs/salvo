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
        depot
            .session_mut()
            .expect("session must be exist")
            .insert(&self.name, format!("{token}.{proof}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BcryptCipher;
    use salvo_core::test::TestClient;
    use salvo_session::{Session, SessionHandler, MemoryStore};

    #[tokio::test]
    async fn test_session_store() {
        let session_store = SessionStore::new();
        let cipher = BcryptCipher::new();
        let mut req = TestClient::get("http://test.com").build();
        let mut depot = Depot::new();
        let mut res = Response::new();
        let session_handler = SessionHandler::new(MemoryStore::new());

        let (token, proof) = cipher.generate();

        // Manually run session handler to create session
        let mut ctrl = salvo_core::FlowCtrl::new(vec![]);
        session_handler.handle(&mut req, &mut depot, &mut res, &mut ctrl).await;

        session_store
            .save(&mut req, &mut depot, &mut res, &token, &proof)
            .await
            .unwrap();

        let loaded = session_store.load(&mut req, &mut depot, &cipher).await;
        assert_eq!(loaded, Some((token, proof)));
    }
}

