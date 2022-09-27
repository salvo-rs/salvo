use salvo_core::http::cookie::{Cookie, Expiration, SameSite};
use salvo_core::{async_trait, FlowCtrl, Request, Response, Handler, Depot};
use std::time;

use super::Flash;

#[async_trait]
pub trait FlashStore: std::fmt::Debug + Send + Sync + 'static {
    async fn load_flash(&self, req: &mut Request) -> Option<Flash>;
    async fn save_flash(&self, flash: Flash, res: &mut Response);
    async fn clear_flash(&self, res: &mut Response);
}

#[derive(Default, Debug)]
pub struct CookieStore {
    pub max_age: time::Duration,
    pub site: SameSite,
    pub http_only: bool,
    pub path: String,
    pub name: String,
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
}
#[async_trait]
impl FlashStore for CookieStore {
    async fn load_flash(&self, req: &mut Request) -> Option<Flash> {
        match req.cookie("_flash") {
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
    async fn save_flash(&self, flash: Flash, res: &mut Response) {
        res.add_cookie(
            Cookie::build(self.name, serde_json::to_string(&flash).unwrap_or_default())
                .max_age(self.max_age)
                .path(self.path.clone())
                .same_site(self.site)
                .http_only(self.http_only)
                .finish(),
        );
    }
    async fn clear_flash(&self, res: &mut Response) {
        res.insert_cookie(
            Cookie::build("_flash", "")
                .max_age(time::Duration::seconds(0))
                .same_site(self.config.site)
                .http_only(self.config.http_only)
                .path(self.config.path.clone())
                .finish(),
        );
    }
}
