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
    <link
      href="{{css_url}}"
      rel="stylesheet"
    />
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

/// Builder for [`ReDoc`].
#[non_exhaustive]
pub struct Builder {
    /// The css url.
    pub css_url: String,
    /// The lib url.
    pub lib_url: String,
    /// The spec url.
    pub spec_url: String,
}

impl Builder {
    /// Create a new [`Builder`] for given path.
    ///
    /// Path argument will expose the ReDoc to the user and should be something that
    /// the underlying application framework / library supports.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use salvo_oapi::redoc::ReDoc;
    /// let doc = ReDoc::builder("/rapidoc/openapi.json");
    /// ```
    pub fn new(spec_url: impl Into<String>) -> Self {
        Self {
            css_url: "https://fonts.googleapis.com/css?family=Montserrat:300,400,700|Roboto:300,400,700".into(),
            lib_url: "https://cdn.redoc.ly/redoc/latest/bundles/redoc.standalone.js".into(),
            spec_url: spec_url.into(),
        }
    }

    /// Set the css url.
    pub fn css_url(mut self, css_url: impl Into<String>) -> Self {
        self.css_url = css_url.into();
        self
    }

    /// Set the lib url.
    pub fn lib_url(mut self, lib_url: impl Into<String>) -> Self {
        self.lib_url = lib_url.into();
        self
    }

    /// Returns the spec url.
    pub fn sepec_url(&self) -> &str {
        &self.spec_url
    }

    /// Consusmes the [`Builder`] and returns [`ReDoc`].
    pub fn build(self) -> ReDoc {
        let Self {
            css_url,
            lib_url,
            spec_url,
        } = self;
        ReDoc {
            html: INDEX_TMPL
                .replace("{{css_url}}", &css_url)
                .replace("{{lib_url}}", &lib_url)
                .replace("{{spec_url}}", &spec_url),
            css_url,
            lib_url,
            spec_url,
        }
    }
}

/// Implements [`Handler`] for serving ReDoc.
#[derive(Clone, Debug)]
pub struct ReDoc {
    css_url: String,
    lib_url: String,
    spec_url: String,
    html: String,
}

impl ReDoc {
    /// Create a new [`Builder`] for given path.
    pub fn builder(spec_url: impl Into<String>) -> Builder {
        Builder::new(spec_url)
    }

    /// Create a new [`ReDoc`] for given path.
    ///
    /// Path argument will expose the ReDoc to the user and should be something that
    /// the underlying application framework / library supports.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use salvo_oapi::redoc::ReDoc;
    /// let doc = ReDoc::new("/rapidoc/openapi.json");
    /// ```
    pub fn new(spec_url: impl Into<String>) -> Self {
        Builder::new(spec_url).build()
    }

    /// Returns the css url.
    pub fn css_url(&self) -> &str {
        &self.css_url
    }

    /// Returns the lib url.
    pub fn lib_url(&self) -> &str {
        &self.lib_url
    }

    /// Returns the spec url.
    pub fn spec_url(&self) -> &str {
        &self.spec_url
    }

    /// Consusmes the [`ReDoc`] and returns [`Router`] with the [`ReDoc`] as handler.
    pub fn into_router(self, path: impl Into<String>) -> Router {
        Router::with_path(path.into()).goal(self)
    }
}

#[async_trait]
impl Handler for ReDoc {
    async fn handle(&self, _req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        res.render(Text::Html(&self.html));
    }
}
