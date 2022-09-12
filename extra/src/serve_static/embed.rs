use std::borrow::Cow;
use std::marker::PhantomData;

use salvo_core::http::header::{CONTENT_TYPE, ETAG, IF_NONE_MATCH};
use salvo_core::http::{Request, Response, StatusCode};
use salvo_core::{async_trait, Depot, FlowCtrl, Handler};
use rust_embed::RustEmbed;

/// Serve static embed assets.
#[derive(Default)]
pub struct StaticEmbed<T> {
    assets: PhantomData<T>,
    index: Option<String>,
}

/// Create a new `StaticEmbed` middleware.
pub fn static_embed<T: RustEmbed>() -> StaticEmbed<T> {
    StaticEmbed {
        assets: PhantomData,
        index: None,
    }
}

/// Create a new `StaticEmbed` middleware with index file.
pub fn static_embed_with_index<T: RustEmbed>(index: impl Into<String>) -> StaticEmbed<T> {
    StaticEmbed {
        assets: PhantomData,
        index: Some(index.into()),
    }
}

impl<T> StaticEmbed<T>
where
    T: RustEmbed + Send + Sync + 'static,
{
    /// Create a new `StaticEmbed`.
    pub fn new() -> Self {
        Self {
            assets: PhantomData,
            index: None,
        }
    }

    /// Create a new `StaticEmbed` with index.
    pub fn with_index(self, index: impl Into<String>) -> Self {
        Self {
            index: Some(index.into()),
            ..self
        }
    }
}
#[async_trait]
impl<T> Handler for StaticEmbed<T>
where
    T: RustEmbed + Send + Sync + 'static,
{
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        let param = req.params().iter().find(|(key, _)| key.starts_with('*'));
        let path = if let Some((_, value)) = param {
            value
        } else {
            self.index.as_deref().unwrap_or_default()
        };
        if path.is_empty() {
            res.set_status_code(StatusCode::NOT_FOUND);
            return;
        }

        match T::get(path) {
            Some(content) => {
                let hash = hex::encode(content.metadata.sha256_hash());
                // if etag is matched, return 304
                if req
                    .headers()
                    .get(IF_NONE_MATCH)
                    .map(|etag| etag.to_str().unwrap_or("000000").eq(&hash))
                    .unwrap_or(false)
                {
                    res.set_status_code(StatusCode::NOT_MODIFIED);
                    return;
                }

                // otherwise, return 200 with etag hash
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                res.headers_mut().insert(ETAG, hash.parse().unwrap());
                res.headers_mut().insert(CONTENT_TYPE, mime.as_ref().parse().unwrap());
                match content.data {
                    Cow::Borrowed(data) => {
                        res.write_body(data).ok();
                    }
                    Cow::Owned(data) => {
                        res.write_body(data).ok();
                    }
                }
            }
            None => res.set_status_code(StatusCode::NOT_FOUND),
        }
    }
}
