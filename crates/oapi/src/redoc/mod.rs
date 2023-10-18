//! This crate implements necessary boiler plate code to serve ReDoc via web server. It
//! works as a bridge for serving the OpenAPI documentation created with [`salvo`][salvo] library in the
//! ReDoc.
//!
//! [salvo]: <https://docs.rs/salvo/>
//!
use salvo_core::writing::Text;
use salvo_core::{async_trait, Depot, FlowCtrl, Handler, Request, Response, Router};

const INDEX_TMPL: &str = r#"
<!DOCTYPE html>
<html>
  <head>
    <title>Redoc</title>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <link href="{{css_url}}" rel="stylesheet"/>
    <style>
      body {
        margin: 0;
        padding: 0;
      }
    </style>
  </head>

  <body>
    <div id="redoc-container"></div>
    <script src="{{lib_url}}"></script>
    <script>
      Redoc.init(
        "{{spec_url}}",
        {},
        document.getElementById("redoc-container")
      );
    </script>
  </body>
</html>
"#;

/// Implements [`Handler`] for serving ReDoc.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct ReDoc {
    /// The lib url path.
    pub lib_url: String,
    /// The spec url path.
    pub spec_url: String,
}

impl ReDoc {
    /// Create a new [`ReDoc`] for given path.
    ///
    /// Path argument will expose the ReDoc to the user and should be something that
    /// the underlying application framework / library supports.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use salvo_oapi::redoc::ReDoc;
    /// let doc = ReDoc::new("/openapi.json");
    /// ```
    pub fn new(spec_url: impl Into<String>) -> Self {
        Self {
            lib_url: "https://cdn.redoc.ly/redoc/latest/bundles/redoc.standalone.js".into(),
            spec_url: spec_url.into(),
        }
    }

    /// Set the lib url path.
    pub fn lib_url(mut self, lib_url: impl Into<String>) -> Self {
      self.lib_url = lib_url.into();
      self
  }

    /// Consusmes the [`ReDoc`] and returns [`Router`] with the [`ReDoc`] as handler.
    pub fn into_router(self, path: impl Into<String>) -> Router {
        Router::with_path(path.into()).goal(self)
    }
}

#[async_trait]
impl Handler for ReDoc {
    async fn handle(&self, _req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        let html = INDEX_TMPL
            .replace("{{lib_url}}", &self.lib_url)
            .replace("{{spec_url}}", &self.spec_url);
        res.render(Text::Html(html));
    }
}
