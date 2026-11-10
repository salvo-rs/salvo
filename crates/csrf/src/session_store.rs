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

#[cfg(all(test, feature = "bcrypt-cipher"))]
mod tests {
    use salvo_core::http::cookie::Cookie;
    use salvo_core::test::TestClient;
    use salvo_session::Session;

    use super::*;
    use crate::{BcryptCipher, CsrfCipher};

    #[tokio::test]
    async fn another_sessions_pair_cannot_be_injected_through_a_cookie() {
        let cipher = BcryptCipher::new().cost(4);
        let store = SessionStore::new();
        let mut attacker = Depot::new();
        attacker.set_session(Session::new());
        let mut victim = Depot::new();
        victim.set_session(Session::new());
        let (token, proof) = cipher.generate();
        let mut req = TestClient::get("https://app.example.test/").build();
        store
            .save(
                &mut req,
                &mut attacker,
                &mut Response::new(),
                &token,
                &proof,
            )
            .await
            .unwrap();
        req.cookies_mut()
            .add(Cookie::new("salvo.csrf", format!("{token}.{proof}")));
        assert_eq!(
            store.load(&mut req, &mut attacker, &cipher).await,
            Some((token, proof))
        );
        assert!(store.load(&mut req, &mut victim, &cipher).await.is_none());
    }
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
    async fn load<C: CsrfCipher>(
        &self,
        _req: &mut Request,
        depot: &mut Depot,
        cipher: &C,
    ) -> Option<(String, String)> {
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
