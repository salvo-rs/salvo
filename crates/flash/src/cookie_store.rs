use salvo_core::http::cookie::time::Duration;
use salvo_core::http::cookie::{Cookie, SameSite};
use salvo_core::{async_trait, Depot, Request, Response};

use super::{Flash, FlashHandler, FlashStore};

/// CookieStore is a `FlashStore` implementation that stores the flash messages in a cookie.
#[derive(Debug)]
pub struct CookieStore {
    max_age: Duration,
    same_site: SameSite,
    http_only: bool,
    path: String,
    name: String,
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
    /// Get cookie name.
    pub fn name(&self) -> &String {
        &self.name
    }
    /// Sets cookie name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }


    /// Get cookie max age.
    pub fn max_age(&self) -> Duration {
        self.max_age
    }
    /// Sets cookie max_age.
    pub fn with_max_age(mut self, max_age: Duration) -> Self {
        self.max_age = max_age;
        self
    }

    /// Get cookie same site.
    pub fn same_site(&self) -> &SameSite {
        &self.same_site
    }
    /// Sets cookie same site.
    pub fn with_same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = same_site;
        self
    }

    /// Get cookie http only.
    pub fn http_only(&self) -> bool {
        self.http_only
    }
    /// Sets cookie http only.
    pub fn with_http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }

    /// Get cookie path.
    pub fn path(&self) -> &String {
        &self.path
    }
    /// Sets cookie path.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
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
            Cookie::build(self.name.clone(), serde_json::to_string(&flash).unwrap_or_default())
                .max_age(self.max_age)
                .path(self.path.clone())
                .same_site(self.same_site)
                .http_only(self.http_only)
                .finish(),
        );
    }
    async fn clear_flash(&self, _depot: &mut Depot, res: &mut Response) {
        res.add_cookie(
            Cookie::build(self.name.clone(), "")
                .max_age(Duration::seconds(0))
                .same_site(self.same_site)
                .http_only(self.http_only)
                .path(self.path.clone())
                .finish(),
        );
    }
}
