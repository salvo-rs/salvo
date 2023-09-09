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
    <script type="module" src="https://unpkg.com/rapidoc/dist/rapidoc-min.js"></script>
  </head>
  <body>
    <rapi-doc spec-url="{{spec_url}}"></rapi-doc>
  </body>
</html>
"#;

/// Implements [`Handler`] for serving RapiDoc.
#[derive(Clone, Debug)]
pub struct RapiDoc {
    spec_url: String,
    html: String,
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
    /// let doc = RapiDoc::new("/rapidoc/openapi.json");
    /// ```
    pub fn new(spec_url: impl Into<String>) -> Self {
        let spec_url = spec_url.into();
        Self {
            html: INDEX_TMPL.replace("{{spec_url}}", &spec_url),
            spec_url,
        }
    }

    /// Returns the spec url.
    pub fn sepec_url(&self) -> &str {
        &self.spec_url
    }

    /// Consusmes the [`RapiDoc`] and returns [`Router`] with the [`RapiDoc`] as handler.
    pub fn into_router(self, path: impl Into<String>) -> Router {
        Router::with_path(path.into()).goal(self)
    }
}

#[async_trait]
impl Handler for RapiDoc {
    async fn handle(&self, _req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        res.render(Text::Html(&self.html));
    }
}