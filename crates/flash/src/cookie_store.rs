use salvo_core::http::cookie::time::Duration;
use salvo_core::http::cookie::{Cookie, SameSite};
use salvo_core::{async_trait, Depot, Request, Response};

use super::{Flash, FlashHandler, FlashStore};

/// CookieStore is a `FlashStore` implementation that stores the flash messages in a cookie.
#[derive(Debug)]
#[non_exhaustive]
pub struct CookieStore {
    /// The cookie max age.
    pub max_age: Duration,
    /// The cookie same site.
    pub same_site: SameSite,
    /// The cookie http only.
    pub http_only: bool,
    /// The cookie path.
    pub path: String,
    /// The cookie name.
    pub name: String,
}
impl Default for CookieStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CookieStore {
    /// Create a new `CookieStore`.
    pub fn new() -> Self {
        Self {
            max_age: Duration::seconds(60),
            same_site: SameSite::Lax,
            http_only: true,
            path: "/".into(),
            name: "salvo.flash".into(),
        }
    }

    /// Sets cookie name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Sets cookie max_age.
    pub fn max_age(mut self, max_age: Duration) -> Self {
        self.max_age = max_age;
        self
    }

    /// Sets cookie same site.
    pub fn same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = same_site;
        self
    }

    /// Sets cookie http only.
    pub fn http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }

    /// Sets cookie path.
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Into `FlashHandler`.
    pub fn into_handler(self) -> FlashHandler<CookieStore> {
        FlashHandler::new(self)
    }
}
#[async_trait]
impl FlashStore for CookieStore {
    async fn load_flash(&self, req: &mut Request, _depot: &mut Depot) -> Option<Flash> {
        match req.cookie(&self.name) {
            None => None,
            Some(cookie) => match serde_json::from_str(cookie.value()) {
                Ok(flash) => Some(flash),
                Err(e) => {
                    tracing::error!(error = ?e, "deserialize flash cookie failed");
                    None
                }
            },
        }
    }
    async fn save_flash(&self, _req: &mut Request, _depot: &mut Depot, res: &mut Response, flash: Flash) {
        res.add_cookie(
            Cookie::build((self.name.clone(), serde_json::to_string(&flash).unwrap_or_default()))
                .max_age(self.max_age)
                .path(self.path.clone())
                .same_site(self.same_site)
                .http_only(self.http_only)
                .build(),
        );
    }
    async fn clear_flash(&self, _depot: &mut Depot, res: &mut Response) {
        res.add_cookie(
            Cookie::build((self.name.clone(), ""))
                .max_age(Duration::seconds(0))
                .same_site(self.same_site)
                .http_only(self.http_only)
                .path(self.path.clone())
                .build(),
        );
    }
}
