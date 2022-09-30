use salvo_core::http::cookie::{time, Cookie, SameSite};
use salvo_core::{async_trait, Request, Depot, Response};

use super::{Flash, FlashStore, FlashHandler};

#[derive(Debug)]
pub struct CookieStore {
    pub max_age: time::Duration,
    pub site: SameSite,
    pub http_only: bool,
    pub path: String,
    pub name: String,
}
impl Default for CookieStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Into<FlashHandler<CookieStore>> for CookieStore {
    fn into(self) -> FlashHandler<CookieStore> {
        FlashHandler::new(self)
    }
}

impl CookieStore {
    /// Create a new `CookieStore`.
    pub fn new() -> Self {
        Self {
            max_age: time::Duration::seconds(60),
            site: SameSite::Lax,
            http_only: true,
            path: "/".into(),
            name: "_flash".into(),
        }
    }

    /// Set cookie max_age.
    pub fn with_max_age(mut self, max_age: time::Duration) -> Self {
        self.max_age = max_age;
        self
    }

    /// Set cookie site.
    pub fn with_site(mut self, site: SameSite) -> Self {
        self.site = site;
        self
    }

    /// Set cookie path.
    pub fn with_http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }

    /// Set cookie path.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Set cookie name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
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
                .same_site(self.site)
                .http_only(self.http_only)
                .finish(),
        );
    }
    async fn clear_flash(&self, _depot: &mut Depot, res: &mut Response) {
        res.add_cookie(
            Cookie::build(self.name.clone(), "")
                .max_age(time::Duration::seconds(0))
                .same_site(self.site)
                .http_only(self.http_only)
                .path(self.path.clone())
                .finish(),
        );
    }
}
