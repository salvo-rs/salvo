use cookie::time::Duration;
use cookie::{Cookie, Expiration, SameSite};
use salvo_core::http::uri::Scheme;
use salvo_core::{Depot, Error, Request, Response};

use crate::CsrfCipher;

use super::CsrfStore;

/// A `CsrfStore` implementation that stores the CSRF proof in a cookie.
#[derive(Debug)]
#[non_exhaustive]
pub struct CookieStore {
    /// CSRF cookie ttl.
    pub ttl: Duration,
    /// CSRF cookie name.
    pub name: String,
    /// CSRF cookie path.
    pub path: String,
    /// CSRF cookie domain.
    pub domain: Option<String>,
}
impl Default for CookieStore {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl CookieStore {
    /// Create a new `CookieStore`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ttl: Duration::days(1),
            name: "salvo.csrf".into(),
            path: "/".into(),
            domain: None,
        }
    }
    /// Sets cookie name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Sets cookie ttl.
    #[must_use]
    pub fn ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Sets cookie path.
    #[must_use]
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Sets cookie domain.
    #[must_use]
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }
}
impl CsrfStore for CookieStore {
    type Error = Error;
    async fn load<C: CsrfCipher>(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        cipher: &C,
    ) -> Option<(String, String)> {
        req.cookie(&self.name)
            .and_then(|c| c.value().split_once('.'))
            .and_then(|(token, proof)| {
                if cipher.verify(token, proof) {
                    Some((token.into(), proof.into()))
                } else {
                    None
                }
            })
    }
    async fn save(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        token: &str,
        proof: &str,
    ) -> Result<(), Self::Error> {
        let secure = req.uri().scheme() == Some(&Scheme::HTTPS);
        let expires = cookie::time::OffsetDateTime::now_utc() + self.ttl;
        let cookie_builder = Cookie::build((self.name.clone(), format!("{token}.{proof}")))
            .http_only(true)
            .same_site(SameSite::Strict)
            .path(self.path.clone())
            .secure(secure)
            .expires(Expiration::DateTime(expires));
        let cookie = if let Some(domain) = &self.domain {
            cookie_builder.domain(domain.clone()).build()
        } else {
            cookie_builder.build()
        };
        res.add_cookie(cookie);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bcrypt_cipher::BcryptCipher;
    use salvo_core::test::TestClient;

    #[tokio::test]
    async fn test_cookie_store() {
        let cipher = BcryptCipher::new();
        let cookie_store = CookieStore::new()
            .name("test_cookie")
            .ttl(Duration::days(1))
            .path("/test")
            .domain("example.com");

        assert_eq!(cookie_store.name, "test_cookie");
        assert_eq!(cookie_store.ttl, Duration::days(1));
        assert_eq!(cookie_store.path, "/test");
        assert_eq!(cookie_store.domain.as_deref(), Some("example.com"));

        let mut req = TestClient::get("https://example.com/test").build();
        let mut depot = Depot::new();
        let mut res = Response::new();

        let (token, proof) = cipher.generate();
        cookie_store
            .save(&mut req, &mut depot, &mut res, &token, &proof)
            .await
            .unwrap();

        let cookie = res.cookies().get("test_cookie").unwrap();
        assert_eq!(cookie.name(), "test_cookie");
        assert_eq!(cookie.path(), Some("/test"));
        assert_eq!(cookie.domain(), Some("example.com"));
        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(cookie.same_site(), Some(SameSite::Strict));
        assert_eq!(cookie.secure(), Some(true));

        req.cookies_mut().add(cookie.clone());

        let loaded = cookie_store.load(&mut req, &mut depot, &cipher).await;
        assert_eq!(loaded, Some((token, proof)));
    }
}
