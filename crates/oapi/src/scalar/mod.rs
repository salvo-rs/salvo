//! This crate implements necessary boiler plate code to serve Scalar via web server. It
//! works as a bridge for serving the OpenAPI documentation created with [`salvo`][salvo] library in the
//! Scalar.
//!
//! [salvo]: <https://docs.rs/salvo/>
//! 
use std::borrow::Cow;

use salvo_core::writing::Text;
use salvo_core::{async_trait, Depot, FlowCtrl, Handler, Request, Response, Router};

const INDEX_TMPL: &str = r#"
<!DOCTYPE html>
<html>
  <head>
    <title>{{title}}</title>
    {{keywords}}
    {{description}}
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <style>
      {{style}}
    </style>
  </head>

  <body>{{header}}
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
    pub title: Cow<'static, str>,
    /// The version of the html page.
    pub keywords: Option<Cow<'static, str>>,
    /// The description of the html page.
    pub description: Option<Cow<'static, str>>,
    /// Custom style for the html page.
    pub style: Option<Cow<'static, str>>,
    /// Custom header for the html page.
    pub header: Option<Cow<'static, str>>,
    /// The lib url path.
    pub lib_url: Cow<'static, str>,
    /// The spec url path.
    pub spec_url: Cow<'static, str>,
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
    pub fn new(spec_url: impl Into<Cow<'static, str>>) -> Self {
        Self {
            title: "Scalar".into(),
            keywords: None,
            description: None,
            style: Some(Cow::from(DEFAULT_STYLE)),
            header: None,
            lib_url: "https://cdn.jsdelivr.net/npm/@scalar/api-reference".into(),
            spec_url: spec_url.into(),
        }
    }

    /// Set title of the html page. The default title is "Scalar".
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
        let style = self
            .style
            .as_ref()
            .map(|s| format!("<style>{}</style>", s))
            .unwrap_or_default();
        let html = INDEX_TMPL
            .replacen("{{lib_url}}", &self.lib_url, 1)
            .replacen("{{spec_url}}", &self.spec_url, 1)
            .replacen("{{title}}", &self.title, 1)
            .replacen("{{keywords}}", &keywords, 1)
            .replacen("{{description}}", &description, 1)
            .replacen("{{style}}", &style, 1)
            .replacen("{{header}}", self.header.as_deref().unwrap_or_default(), 1);
        res.render(Text::Html(html));
    }
}

const DEFAULT_STYLE: &'static str = r#":root {
    --theme-font: 'Inter', var(--system-fonts);
  }
  /* basic theme */
  .light-mode {
    --theme-color-1: #2c3d50;
    --theme-color-2: #38495c;
    --theme-color-3: #445569;
    --theme-color-accent: #3faf7c;
  
    --theme-background-1: #fff;
    --theme-background-2: #f6f6f6;
    --theme-background-3: #e7e7e7;
    --theme-background-accent: #8ab4f81f;
  
    --theme-border-color: rgba(0, 0, 0, 0.1);
  }
  .dark-mode {
    --theme-color-1: rgb(150, 167, 183, 1);
    --theme-color-2: rgba(150, 167, 183, 0.72);
    --theme-color-3: rgba(150, 167, 183, 0.54);
    --theme-color-accent: #329066;
  
    --theme-background-1: #22272e;
    --theme-background-2: #282c34;
    --theme-background-3: #343841;
    --theme-background-accent: #3290661f;
  
    --theme-border-color: rgba(255, 255, 255, 0.1);
  }
  /* Document header */
  .light-mode .t-doc__header {
    --header-background-1: var(--theme-background-1);
    --header-border-color: var(--theme-border-color);
    --header-color-1: var(--theme-color-1);
    --header-color-2: var(--theme-color-2);
    --header-background-toggle: var(--theme-color-3);
    --header-call-to-action-color: var(--theme-color-accent);
  }
  
  .dark-mode .t-doc__header {
    --header-background-1: var(--theme-background-1);
    --header-border-color: var(--theme-border-color);
    --header-color-1: var(--theme-color-1);
    --header-color-2: var(--theme-color-2);
    --header-background-toggle: var(--theme-color-3);
    --header-call-to-action-color: var(--theme-color-accent);
  }
  /* Document Sidebar */
  .light-mode .t-doc__sidebar,
  .dark-mode .t-doc__sidebar {
    --sidebar-background-1: var(--theme-background-1);
    --sidebar-item-hover-color: var(--theme-color-accent);
    --sidebar-item-hover-background: transparent;
    --sidebar-item-active-background: transparent;
    --sidebar-border-color: var(--theme-border-color);
    --sidebar-color-1: var(--theme-color-1);
    --sidebar-color-2: var(--theme-color-2);
    --sidebar-color-active: var(--theme-color-accent);
    --sidebar-search-background: transparent;
    --sidebar-search-border-color: var(--theme-border-color);
    --sidebar-search--color: var(--theme-color-3);
  }
  .light-mode .t-doc__sidebar .active_page.sidebar-heading,
  .dark-mode .t-doc__sidebar .active_page.sidebar-heading {
    background: transparent !important;
    box-shadow: inset 3px 0 0 var(--theme-color-accent);
  }
  
  /* advanced */
  .light-mode {
    --theme-button-1: rgb(49 53 56);
    --theme-button-1-color: #fff;
    --theme-button-1-hover: rgb(28 31 33);
  
    --theme-color-green: #069061;
    --theme-color-red: #ef0006;
    --theme-color-yellow: #edbe20;
    --theme-color-blue: #0082d0;
    --theme-color-orange: #fb892c;
    --theme-color-purple: #5203d1;
  
    --theme-scrollbar-color: rgba(0, 0, 0, 0.18);
    --theme-scrollbar-color-active: rgba(0, 0, 0, 0.36);
  }
  .dark-mode {
    --theme-button-1: #f6f6f6;
    --theme-button-1-color: #000;
    --theme-button-1-hover: #e7e7e7;
  
    --theme-color-green: #00b648;
    --theme-color-red: #dc1b19;
    --theme-color-yellow: #ffc90d;
    --theme-color-blue: #4eb3ec;
    --theme-color-orange: #ff8d4d;
    --theme-color-purple: #b191f9;
  
    --theme-scrollbar-color: var(--theme-color-accent);
    --theme-scrollbar-color-active: var(--theme-color-accent);
  }
  body {margin: 0; padding: 0;}
  .dark-mode .show-api-client-button span,
  .light-mode .show-api-client-button span,
  .light-mode .show-api-client-button svg,
  .dark-mode .show-api-client-button svg {
    color: white;
  }
  .t-doc__header .header-item-logo {
    height: 32px;
  }
"#;
