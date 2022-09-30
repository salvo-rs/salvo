use cookie::time::Duration;
use cookie::{Cookie, Expiration, SameSite};
use salvo_core::http::uri::Scheme;
use salvo_core::{async_trait, Depot, Error, Request, Response};

use super::CsrfStore;

#[derive(Debug)]
pub struct CookieStore {
    /// CSRF Cookie ttl
    pub ttl: Duration,
    pub name: String,
    pub path: String,
    pub domain: Option<String>,
}

impl CookieStore {
    /// Create a new `CookieStore`.
    pub fn new() -> Self {
        Self {
            ttl: Duration::days(1),
            name: "salvo.csrf".into(),
            path: "/".into(),
            domain: None,
        }
    }

    /// Set cookie ttl.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl.into();
        self
    }

    /// Set cookie name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set cookie path.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Set cookie domain.
    pub fn with_domain(mut self, domain: impl Into<Option<String>>) -> Self {
        self.domain = domain.into();
        self
    }
}
#[async_trait]
impl CsrfStore for CookieStore {
    type Error = Error;
    async fn load_secret(&self, req: &mut Request, _depot: &mut Depot) -> Option<Vec<u8>> {
        req.cookie(&self.name).and_then(|c| base64::decode(c.value()).ok())
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
        res.add_cookie(
            Cookie::build(self.name.clone(), base64::encode(&secret))
                .http_only(true)
                .same_site(SameSite::Strict)
                .path(self.path.clone())
                .secure(secure)
                .expires(Expiration::DateTime(expires))
                .finish(),
        );
        Ok(())
    }
}
