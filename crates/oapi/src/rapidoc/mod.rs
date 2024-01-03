//! This crate implements necessary boiler plate code to serve RapiDoc via web server. It
//! works as a bridge for serving the OpenAPI documentation created with [`salvo`][salvo] library in the
//! RapiDoc.
//!
//! [salvo]: <https://docs.rs/salvo/>
//!
use std::borrow::Cow;

use salvo_core::writing::Text;
use salvo_core::{async_trait, Depot, FlowCtrl, Handler, Request, Response, Router};

const INDEX_TMPL: &str = r#"
<!doctype html>
<html>
  <head>
    <title>{{title}}</title>
    {{keywords}}
    {{description}}
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
    /// The title of the html page. The default title is "RapiDoc".
    pub title: Cow<'static, str>,
    /// The version of the html page.
    pub keywords: Option<Cow<'static, str>>,
    /// The description of the html page.
    pub description: Option<Cow<'static, str>>,
    /// The lib url path.
    pub lib_url: Cow<'static, str>,
    /// The spec url path.
    pub spec_url: Cow<'static, str>,
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
    pub fn new(spec_url: impl Into<Cow<'static, str>>) -> Self {
        Self {
            title: "RapiDoc".into(),
            keywords: None,
            description: None,
            lib_url: "https://unpkg.com/rapidoc/dist/rapidoc-min.js".into(),
            spec_url: spec_url.into(),
        }
    }

    /// Set title of the html page. The default title is "RapiDoc".
    pub fn title(mut self, title: impl Into<Cow<'static, str>>) -> Self {
        self.title = title.into();
        self
    }

    /// Set keywords of the html page.
    pub fn keywords(mut self, keywords: impl Into<Cow<'static, str>>) -> Self {
        self.keywords = Some(keywords.into());
        self
    }

    /// Set description of the html page.
    pub fn description(mut self, description: impl Into<Cow<'static, str>>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the lib url path.
    pub fn lib_url(mut self, lib_url: impl Into<Cow<'static, str>>) -> Self {
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
            .replacen("{{spec_url}}", &self.spec_url, 1)
            .replacen("{{lib_url}}", &self.lib_url, 1)
            .replacen("{{description}}", &description, 1)
            .replacen("{{keywords}}", &keywords, 1)
            .replacen("{{title}}", &self.title, 1);
        res.render(Text::Html(html));
    }
}
