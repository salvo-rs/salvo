//! This crate implements necessary boiler plate code to serve RapiDoc via web server. It
//! works as a bridge for serving the OpenAPI documentation created with [`salvo`][salvo] library in the
//! RapiDoc.
//!
//! [salvo]: <https://docs.rs/salvo/>
//!
use salvo_core::writing::Text;
use salvo_core::{async_trait, Depot, FlowCtrl, Handler, Request, Response, Router};

const INDEX_TMPL: &str = r#"
<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <script type="module" src="{{lib_url}}"></script>
  </head>
  <body>
    <rapi-doc spec-url="{{spec_url}}"></rapi-doc>
  </body>
</html>
"#;

/// Implements [`Handler`] for serving RapiDoc.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct RapiDoc {
    /// The lib url path.
    pub lib_url: String,
    /// The spec url path.
    pub spec_url: String,
}
impl RapiDoc {
    /// Create a new [`RapiDoc`] for given path.
    ///
    /// Path argument will expose the RapiDoc to the user and should be something that
    /// the underlying application framework / library supports.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use salvo_oapi::rapidoc::RapiDoc;
    /// let doc = RapiDoc::new("/openapi.json");
    /// ```
    pub fn new(spec_url: impl Into<String>) -> Self {
        Self {
            lib_url: "https://unpkg.com/rapidoc/dist/rapidoc-min.js".into(),
            spec_url: spec_url.into(),
        }
    }

    /// Set the lib url path.
    pub fn lib_url(mut self, lib_url: impl Into<String>) -> Self {
        self.lib_url = lib_url.into();
        self
    }

    /// Consusmes the [`RapiDoc`] and returns [`Router`] with the [`RapiDoc`] as handler.
    pub fn into_router(self, path: impl Into<String>) -> Router {
        Router::with_path(path.into()).goal(self)
    }
}

#[async_trait]
impl Handler for RapiDoc {
    async fn handle(&self, _req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        let html = INDEX_TMPL
            .replace("{{lib_url}}", &self.lib_url)
            .replace("{{spec_url}}", &self.spec_url);
        res.render(Text::Html(html));
    }
}
