use base64::engine::{general_purpose, Engine};
use cookie::time::Duration;
use cookie::{Cookie, Expiration, SameSite};
use salvo_core::http::uri::Scheme;
use salvo_core::{async_trait, Depot, Error, Request, Response};

use super::CsrfStore;

/// CookieStore is a `CsrfStore` implementation that stores the CSRF secret in a cookie.
#[derive(Debug)]
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
    pub fn new() -> Self {
        Self {
            ttl: Duration::days(1),
            name: "salvo.csrf.secret".into(),
            path: "/".into(),
            domain: None,
        }
    }
    /// Sets cookie name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Sets cookie ttl.
    pub fn ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Sets cookie path.
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Sets cookie domain.
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }
}
#[async_trait]
impl CsrfStore for CookieStore {
    type Error = Error;
    async fn load_secret(&self, req: &mut Request, _depot: &mut Depot) -> Option<Vec<u8>> {
        req.cookie(&self.name)
            .and_then(|c| general_purpose::STANDARD.decode(c.value()).ok())
    }
    async fn save_secret(
        &self,
        req: &mut Request,
        _depot: &mut Depot,
        res: &mut Response,
        secret: &[u8],
    ) -> Result<(), Self::Error> {
        let secure = req.uri().scheme() == Some(&Scheme::HTTPS);
        let expires = cookie::time::OffsetDateTime::now_utc() + self.ttl;
        let cookie_builder = Cookie::build(self.name.clone(), general_purpose::STANDARD.encode(secret))
            .http_only(true)
            .same_site(SameSite::Strict)
            .path(self.path.clone())
            .secure(secure)
            .expires(Expiration::DateTime(expires));
        let cookie = if let Some(domain) = &self.domain {
            cookie_builder.domain(domain.clone()).finish()
        } else {
            cookie_builder.finish()
        };
        res.add_cookie(cookie);
        Ok(())
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use salvo_core::Depot;
//     use salvo_core::Request;
//     use salvo_core::Response;

//     #[tokio::test]
//     async fn test_cookie_store() {
//         let cookie_store = CookieStore::new()
//             .with_name("test_cookie")
//             .with_ttl(Duration::days(1))
//             .with_path("/test")
//             .with_domain("example.com");

//         assert_eq!(cookie_store.name(), "test_cookie");
//         assert_eq!(cookie_store.ttl(), Duration::days(1));
//         assert_eq!(cookie_store.path(), "/test");
//         assert_eq!(cookie_store.domain(), Some(&"example.com".to_string()));

//         let mut req = Request::new();
//         let mut depot = Depot::new();
//         let mut res = Response::new();

//         let secret = vec![1, 2, 3, 4, 5];
//         cookie_store
//             .save_secret(&mut req, &mut depot, &mut res, &secret)
//             .await
//             .unwrap();

//         let loaded_secret = cookie_store.load_secret(&mut req, &mut depot).await;
//         assert_eq!(loaded_secret, Some(secret));
//     }
// }
