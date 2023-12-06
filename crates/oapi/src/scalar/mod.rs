//! This crate implements necessary boiler plate code to serve Scalar via web server. It
//! works as a bridge for serving the OpenAPI documentation created with [`salvo`][salvo] library in the
//! Scalar.
//!
//! [salvo]: <https://docs.rs/salvo/>
//!
use salvo_core::writing::Text;
use salvo_core::{async_trait, Depot, FlowCtrl, Handler, Request, Response, Router};

const INDEX_TMPL: &str = r#"
<!DOCTYPE html>
<html>
  <head>
    <title>{{title}}</title>
    {{keywords}}
    {{description}}
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style>
      body {
        margin: 0;
        padding: 0;
      }
    </style>
  </head>

  <body>
    <script id="api-reference" data-url="{{spec_url}}"></script>
    <script src="{{lib_url}}"></script>
  </body>
</html>

"#;

/// Implements [`Handler`] for serving Scalar.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Scalar {
    /// The title of the html page. The default title is "Scalar".
    pub title: String,
    /// The version of the html page.
    pub keywords: Option<String>,
    /// The description of the html page.
    pub description: Option<String>,
    /// The lib url path.
    pub lib_url: String,
    /// The spec url path.
    pub spec_url: String,
}
impl Scalar {
    /// Create a new [`Scalar`] for given path.
    ///
    /// Path argument will expose the Scalar to the user and should be something that
    /// the underlying application framework / library supports.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use salvo_oapi::scalar::Scalar;
    /// let doc = Scalar::new("/openapi.json");
    /// ```
    pub fn new(spec_url: impl Into<String>) -> Self {
        Self {
            title: "Scalar".into(),
            keywords: None,
            description: None,
            lib_url: "https://cdn.jsdelivr.net/npm/@scalar/api-reference".into(),
            spec_url: spec_url.into(),
        }
    }

    /// Set title of the html page. The default title is "Scalar".
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Set keywords of the html page.
    pub fn keywords(mut self, keywords: impl Into<String>) -> Self {
        self.keywords = Some(keywords.into());
        self
    }

    /// Set description of the html page.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the lib url path.
    pub fn lib_url(mut self, lib_url: impl Into<String>) -> Self {
        self.lib_url = lib_url.into();
        self
    }

    /// Consusmes the [`Scalar`] and returns [`Router`] with the [`Scalar`] as handler.
    pub fn into_router(self, path: impl Into<String>) -> Router {
        Router::with_path(path.into()).goal(self)
    }
}
#[async_trait]
impl Handler for Scalar {
    async fn handle(&self, _req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        let keywords = self
            .keywords
            .as_ref()
            .map(|s| {
                format!(
                    "<meta name=\"keywords\" content=\"{}\">",
                    s.split(',').map(|s| s.trim()).collect::<Vec<_>>().join(",")
                )
            })
            .unwrap_or_default();
        let description = self
            .description
            .as_ref()
            .map(|s| format!("<meta name=\"description\" content=\"{}\">", s))
            .unwrap_or_default();
        let html = INDEX_TMPL
            .replacen("{{lib_url}}", &self.lib_url, 1)
            .replacen("{{spec_url}}", &self.spec_url, 1)
            .replacen("{{title}}", &self.title, 1)
            .replacen("{{keywords}}", &keywords, 1)
            .replacen("{{description}}", &description, 1);
        res.render(Text::Html(html));
    }
}
