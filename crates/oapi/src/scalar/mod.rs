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
    <title>Scalar</title>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <link
      href="https://fonts.googleapis.com/css?family=Montserrat:300,400,700|Roboto:300,400,700"
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
    <script id="api-reference" data-url="{{spec_url}}"></script>
    <script src="https://www.unpkg.com/@scalar/api-reference"></script>
  </body>
</html>

"#;

/// Implements [`Handler`] for serving ReDoc.
#[derive(Clone, Debug)]
pub struct Scalar {
    spec_url: String,
    html: String,
}
impl Scalar {
    /// Create a new [`ReDoc`] for given path.
    ///
    /// Path argument will expose the ReDoc to the user and should be something that
    /// the underlying application framework / library supports.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use salvo_oapi::scalar::Scalar;
    /// let doc = Scalar::new("/openapi.json");
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

    /// Consusmes the [`Scalar`] and returns [`Router`] with the [`Scalar`] as handler.
    pub fn into_router(self, path: impl Into<String>) -> Router {
        Router::with_path(path.into()).goal(self)
    }
}

#[async_trait]
impl Handler for Scalar {
    async fn handle(&self, _req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        res.render(Text::Html(&self.html));
    }
}