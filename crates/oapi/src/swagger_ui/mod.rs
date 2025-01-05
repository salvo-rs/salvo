//! This crate implements necessary boiler plate code to serve Swagger UI via web server. It
//! works as a bridge for serving the OpenAPI documentation created with [`salvo`][salvo] library in the
//! Swagger UI.
//!
//! [salvo]: <https://docs.rs/salvo/>
//!
use std::borrow::Cow;

mod config;
pub mod oauth;
pub use config::Config;
pub use oauth::Config as OauthConfig;
use rust_embed::RustEmbed;
use salvo_core::http::uri::{Parts as UriParts, Uri};
use salvo_core::http::{header, HeaderValue, ResBody, StatusError};
use salvo_core::writing::Redirect;
use salvo_core::{async_trait, Depot, Error, FlowCtrl, Handler, Request, Response, Router};
use serde::Serialize;

#[derive(RustEmbed)]
#[folder = "src/swagger_ui/v5.18.3"]
struct SwaggerUiDist;

const INDEX_TMPL: &str = r#"
<!DOCTYPE html>
<html charset="UTF-8">
  <head>
    <meta charset="UTF-8">
    <title>{{title}}</title>
    {{keywords}}
    {{description}}
    <link rel="stylesheet" type="text/css" href="./swagger-ui.css" />
    <style>
    html {
        box-sizing: border-box;
        overflow: -moz-scrollbars-vertical;
        overflow-y: scroll;
    }
    *,
    *:before,
    *:after {
        box-sizing: inherit;
    }
    body {
        margin: 0;
        background: #fafafa;
    }
    </style>
  </head>

  <body>
    <div id="swagger-ui"></div>
    <script src="./swagger-ui-bundle.js" charset="UTF-8"></script>
    <script src="./swagger-ui-standalone-preset.js" charset="UTF-8"></script>
    <script>
    window.onload = function() {
        let config = {
            dom_id: '#swagger-ui',
            deepLinking: true,
            presets: [
              SwaggerUIBundle.presets.apis,
              SwaggerUIStandalonePreset
            ],
            plugins: [
              SwaggerUIBundle.plugins.DownloadUrl
            ],
            layout: "StandaloneLayout"
          };
        window.ui = SwaggerUIBundle(Object.assign(config, {{config}}));
        //{{oauth}}
    };
    </script>
  </body>
</html>
"#;

/// Implements [`Handler`] for serving Swagger UI.
#[derive(Clone, Debug)]
pub struct SwaggerUi {
    config: Config<'static>,
    /// The title of the html page. The default title is "Swagger UI".
    pub title: Cow<'static, str>,
    /// The keywords of the html page.
    pub keywords: Option<Cow<'static, str>>,
    /// The description of the html page.
    pub description: Option<Cow<'static, str>>,
}
impl SwaggerUi {
    /// Create a new [`SwaggerUi`] for given path.
    ///
    /// Path argument will expose the Swagger UI to the user and should be something that
    /// the underlying application framework / library supports.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use salvo_oapi::swagger_ui::SwaggerUi;
    /// let swagger = SwaggerUi::new("/swagger-ui/{_:.*}");
    /// ```
    pub fn new(config: impl Into<Config<'static>>) -> Self {
        Self {
            config: config.into(),
            title: "Swagger UI".into(),
            keywords: None,
            description: None,
        }
    }

    /// Set title of the html page. The default title is "Swagger UI".
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

    /// Add api doc [`Url`] into [`SwaggerUi`].
    ///
    /// Calling this again will add another url to the Swagger UI.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use salvo_oapi::swagger_ui::SwaggerUi;
    /// # use salvo_oapi::OpenApi;
    ///
    /// let swagger = SwaggerUi::new("/api-doc/openapi.json")
    ///     .url("/api-docs/openapi2.json");
    /// ```
    pub fn url<U: Into<Url<'static>>>(mut self, url: U) -> Self {
        self.config.urls.push(url.into());
        self
    }

    /// Add multiple [`Url`]s to Swagger UI.
    ///
    /// Takes one [`Vec`] argument containing tuples of [`Url`] and [OpenApi][crate::OpenApi].
    ///
    /// Situations where this comes handy is when there is a need or wish to separate different parts
    /// of the api to separate api docs.
    ///
    /// # Examples
    ///
    /// Expose multiple api docs via Swagger UI.
    /// ```rust
    /// # use salvo_oapi::swagger_ui::{SwaggerUi, Url};
    /// # use salvo_oapi::OpenApi;
    ///
    /// let swagger = SwaggerUi::new("/swagger-ui/{_:.*}")
    ///     .urls(
    ///       vec![
    ///          (Url::with_primary("api doc 1", "/api-docs/openapi.json", true)),
    ///          (Url::new("api doc 2", "/api-docs/openapi2.json"))
    ///     ]
    /// );
    /// ```
    pub fn urls(mut self, urls: Vec<Url<'static>>) -> Self {
        self.config.urls = urls;
        self
    }

    /// Add oauth [`oauth::Config`] into [`SwaggerUi`].
    ///
    /// Method takes one argument which exposes the [`oauth::Config`] to the user.
    ///
    /// # Examples
    ///
    /// Enable pkce with default client_id.
    /// ```rust
    /// # use salvo_oapi::swagger_ui::{SwaggerUi, oauth};
    /// # use salvo_oapi::OpenApi;
    ///
    /// let swagger = SwaggerUi::new("/swagger-ui/{_:.*}")
    ///     .url("/api-docs/openapi.json")
    ///     .oauth(oauth::Config::new()
    ///         .client_id("client-id")
    ///         .scopes(vec![String::from("openid")])
    ///         .use_pkce_with_authorization_code_grant(true)
    ///     );
    /// ```
    pub fn oauth(mut self, oauth: oauth::Config) -> Self {
        self.config.oauth = Some(oauth);
        self
    }

    /// Consusmes the [`SwaggerUi`] and returns [`Router`] with the [`SwaggerUi`] as handler.
    pub fn into_router(self, path: impl Into<String>) -> Router {
        Router::with_path(format!("{}/{{**}}", path.into())).goal(self)
    }
}

#[inline]
pub(crate) fn redirect_to_dir_url(req_uri: &Uri, res: &mut Response) {
    let UriParts {
        scheme,
        authority,
        path_and_query,
        ..
    } = req_uri.clone().into_parts();
    let mut builder = Uri::builder();
    if let Some(scheme) = scheme {
        builder = builder.scheme(scheme);
    }
    if let Some(authority) = authority {
        builder = builder.authority(authority);
    }
    if let Some(path_and_query) = path_and_query {
        if let Some(query) = path_and_query.query() {
            builder = builder.path_and_query(format!("{}/?{}", path_and_query.path(), query));
        } else {
            builder = builder.path_and_query(format!("{}/", path_and_query.path()));
        }
    }
    match builder.build() {
        Ok(redirect_uri) => res.render(Redirect::found(redirect_uri)),
        Err(e) => {
            tracing::error!(error = ?e, "failed to build redirect uri");
            res.render(StatusError::internal_server_error());
        }
    }
}

#[async_trait]
impl Handler for SwaggerUi {
    async fn handle(&self, req: &mut Request, _depot: &mut Depot, res: &mut Response, _ctrl: &mut FlowCtrl) {
        let path = req.params().tail().unwrap_or_default();
        // Redirect to dir url if path is empty and not end with '/'
        if path.is_empty() && !req.uri().path().ends_with('/') {
            redirect_to_dir_url(req.uri(), res);
            return;
        }

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
        match serve(path, &self.title, &keywords, &description, &self.config) {
            Ok(Some(file)) => {
                res.headers_mut()
                    .insert(header::CONTENT_TYPE, HeaderValue::from_str(&file.content_type).expect("content type parse failed"));
                res.body(ResBody::Once(file.bytes.to_vec().into()));
            }
            Ok(None) => {
                tracing::warn!(path, "swagger ui file not found");
                res.render(StatusError::not_found());
            }
            Err(e) => {
                tracing::error!(error = ?e, path, "failed to fetch swagger ui file");
                res.render(StatusError::internal_server_error());
            }
        }
    }
}

/// Rust type for Swagger UI url configuration object.
#[non_exhaustive]
#[derive(Default, Serialize, Clone, Debug)]
pub struct Url<'a> {
    name: Cow<'a, str>,
    url: Cow<'a, str>,
    #[serde(skip)]
    primary: bool,
}

impl<'a> Url<'a> {
    /// Create new [`Url`].
    ///
    /// Name is shown in the select dropdown when there are multiple docs in Swagger UI.
    ///
    /// Url is path which exposes the OpenAPI doc.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use salvo_oapi::swagger_ui::Url;
    /// let url = Url::new("My Api", "/api-docs/openapi.json");
    /// ```
    pub fn new(name: &'a str, url: &'a str) -> Self {
        Self {
            name: Cow::Borrowed(name),
            url: Cow::Borrowed(url),
            ..Default::default()
        }
    }

    /// Create new [`Url`] with primary flag.
    ///
    /// Primary flag allows users to override the default behavior of the Swagger UI for selecting the primary
    /// doc to display. By default when there are multiple docs in Swagger UI the first one in the list
    /// will be the primary.
    ///
    /// Name is shown in the select dropdown when there are multiple docs in Swagger UI.
    ///
    /// Url is path which exposes the OpenAPI doc.
    ///
    /// # Examples
    ///
    /// Set "My Api" as primary.
    /// ```rust
    /// # use salvo_oapi::swagger_ui::Url;
    /// let url = Url::with_primary("My Api", "/api-docs/openapi.json", true);
    /// ```
    pub fn with_primary(name: &'a str, url: &'a str, primary: bool) -> Self {
        Self {
            name: Cow::Borrowed(name),
            url: Cow::Borrowed(url),
            primary,
        }
    }
}

impl<'a> From<&'a str> for Url<'a> {
    fn from(url: &'a str) -> Self {
        Self {
            url: Cow::Borrowed(url),
            ..Default::default()
        }
    }
}

impl From<String> for Url<'_> {
    fn from(url: String) -> Self {
        Self {
            url: Cow::Owned(url),
            ..Default::default()
        }
    }
}

impl From<Cow<'static, str>> for Url<'_> {
    fn from(url: Cow<'static, str>) -> Self {
        Self {
            url,
            ..Default::default()
        }
    }
}

/// Represents servable file of Swagger UI. This is used together with [`serve`] function
/// to serve Swagger UI files via web server.
#[non_exhaustive]
pub struct SwaggerFile<'a> {
    /// Content of the file as [`Cow`] [`slice`] of bytes.
    pub bytes: Cow<'a, [u8]>,
    /// Content type of the file e.g `"text/xml"`.
    pub content_type: String,
}

/// User friendly way to serve Swagger UI and its content via web server.
///
/// * **path** Should be the relative path to Swagger UI resource within the web server.
/// * **config** Swagger [`Config`] to use for the Swagger UI.
///
/// Typically this function is implemented _**within**_ handler what serves the Swagger UI. Handler itself must
/// match to user defined path that points to the root of the Swagger UI and match everything relatively
/// from the root of the Swagger UI _**(tail path)**_. The relative path from root of the Swagger UI
/// is used to serve [`SwaggerFile`]s. If Swagger UI is served from path `/swagger-ui/` then the `tail`
/// is everything under the `/swagger-ui/` prefix.
///
/// _There are also implementations in [examples of salvo repository][examples]._
///
/// [examples]: https://github.com/salvo-rs/salvo/tree/master/examples
pub fn serve<'a>(
    path: &str,
    title: &str,
    keywords: &str,
    description: &str,
    config: &Config<'a>,
) -> Result<Option<SwaggerFile<'a>>, Error> {
    let path = if path.is_empty() || path == "/" {
        "index.html"
    } else {
        path
    };

    let bytes = if path == "index.html" {
        let config_json = serde_json::to_string(&config)?;

        // Replace {{config}} with pretty config json and remove the curly brackets `{ }` from beginning and the end.
        let mut index = INDEX_TMPL
            .replacen("{{config}}", &config_json, 1)
            .replacen("{{description}}", description, 1)
            .replacen("{{keywords}}", keywords, 1)
            .replacen("{{title}}", title, 1);

        if let Some(oauth) = &config.oauth {
            let oauth_json = serde_json::to_string(oauth)?;
            index = index.replace("//{{oauth}}", &format!("window.ui.initOAuth({});", &oauth_json));
        }
        Some(Cow::Owned(index.as_bytes().to_vec()))
    } else {
        SwaggerUiDist::get(path).map(|f| f.data)
    };
    let file = bytes.map(|bytes| SwaggerFile {
        bytes,
        content_type: mime_infer::from_path(path).first_or_octet_stream().to_string(),
    });

    Ok(file)
}
