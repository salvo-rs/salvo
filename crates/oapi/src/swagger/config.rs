use std::borrow::Cow;

use serde::Serialize;

use super::oauth;
use crate::swagger::Url;

const SWAGGER_STANDALONE_LAYOUT: &str = "StandaloneLayout";
const SWAGGER_BASE_LAYOUT: &str = "BaseLayout";

/// Object used to alter Swagger UI settings.
///
/// Config struct provides [Swagger UI configuration](https://github.com/swagger-api/swagger-ui/blob/master/docs/usage/configuration.md)
/// for settings which could be altered with **docker variables**.
///
/// # Examples
///
/// In simple case create config directly from url that points to the api doc json.
/// ```rust
/// # use salvo_oapi::swagger::Config;
/// let config = Config::from("/api-doc.json");
/// ```
///
/// If there is multiple api docs to serve config can be also directly created with [`Config::new`]
/// ```rust
/// # use salvo_oapi::swagger::Config;
/// let config = Config::new(["/api-docs/openapi1.json", "/api-docs/openapi2.json"]);
/// ```
///
/// Or same as above but more verbose syntax.
/// ```rust
/// # use salvo_oapi::swagger::{Config, Url};
/// let config = Config::new([
///     Url::new("api1", "/api-docs/openapi1.json"),
///     Url::new("api2", "/api-docs/openapi2.json")
/// ]);
/// ```
///
/// With oauth config.
/// ```rust
/// # use salvo_oapi::swagger::{Config, oauth};
/// let config = Config::with_oauth_config(
///     ["/api-docs/openapi1.json", "/api-docs/openapi2.json"],
///     oauth::Config::new(),
/// );
/// ```
#[non_exhaustive]
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Config<'a> {
    /// Url to fetch external configuration from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) config_url: Option<String>,

    /// Id of the DOM element where `Swagger UI` will put it's user interface.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "dom_id")]
    pub(crate) dom_id: Option<String>,

    /// [`Url`] the Swagger UI is serving.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) url: Option<String>,

    /// Name of the primary url if any.
    #[serde(skip_serializing_if = "Option::is_none", rename = "urls.primaryName")]
    pub(crate) urls_primary_name: Option<String>,

    /// [`Url`]s the Swagger UI is serving.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) urls: Vec<Url<'a>>,

    /// Enables overriding configuration parameters with url query parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) query_config_enabled: Option<bool>,

    /// Controls whether [deep linking](https://github.com/swagger-api/swagger-ui/blob/master/docs/usage/deep-linking.md)
    /// is enabled in OpenAPI spec.
    ///
    /// Deep linking automatically scrolls and expands UI to given url fragment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) deep_linking: Option<bool>,

    /// Controls whether operation id is shown in the operation list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) display_operation_id: Option<bool>,

    /// Default models expansion depth; -1 will completely hide the models.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default_models_expand_depth: Option<isize>,

    /// Default model expansion depth from model example section.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default_model_expand_depth: Option<isize>,

    /// Defines how models is show when API is first rendered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default_model_rendering: Option<String>,

    /// Define whether request duration in milliseconds is displayed for "Try it out" requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) display_request_duration: Option<bool>,

    /// Controls default expansion for operations and tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) doc_expansion: Option<String>,

    /// Defines is filtering of tagged operations allowed with edit box in top bar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) filter: Option<bool>,

    /// Controls how many tagged operations are shown. By default all operations are shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_displayed_tags: Option<usize>,

    /// Defines whether extensions are shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) show_extensions: Option<bool>,

    /// Defines whether common extensions are shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) show_common_extensions: Option<bool>,

    /// Defines whether "Try it out" section should be enabled by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) try_it_out_enabled: Option<bool>,

    /// Defines whether request snippets section is enabled. If disabled legacy curl snipped
    /// will be used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_snippets_enabled: Option<bool>,

    /// Oauth redirect url.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) oauth2_redirect_url: Option<String>,

    /// Defines whether request mutated with `requestInterceptor` will be used to produce curl command
    /// in the UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) show_mutated_request: Option<bool>,

    /// Define supported http request submit methods.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) supported_submit_methods: Option<Vec<String>>,

    /// Define validator url which is used to validate the Swagger spec. By default the validator swagger.io's
    /// online validator is used. Setting this to none will disable spec validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) validator_url: Option<String>,

    /// Enables passing credentials to CORS requests as defined
    /// [fetch standards](https://fetch.spec.whatwg.org/#credentials).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) with_credentials: Option<bool>,

    /// Defines whether authorizations is persisted throughout browser refresh and close.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) persist_authorization: Option<bool>,

    /// [`oauth::Config`] the Swagger UI is using for auth flow.
    #[serde(skip)]
    pub(crate) oauth: Option<oauth::Config>,

    /// The layout of Swagger UI uses, default is `"StandaloneLayout"`
    pub(crate) layout: &'a str,
}

impl<'a> Config<'a> {
    fn new_<I: IntoIterator<Item = U>, U: Into<Url<'a>>>(urls: I, oauth_config: Option<oauth::Config>) -> Self {
        let urls = urls.into_iter().map(Into::into).collect::<Vec<Url<'a>>>();
        let urls_len = urls.len();

        Self {
            oauth: oauth_config,
            ..if urls_len == 1 {
                Self::new_config_with_single_url(urls)
            } else {
                Self::new_config_with_multiple_urls(urls)
            }
        }
    }

    fn new_config_with_multiple_urls(urls: Vec<Url<'a>>) -> Self {
        let primary_name = urls.iter().find(|url| url.primary).map(|url| url.name.to_string());

        Self {
            urls_primary_name: primary_name,
            urls: urls
                .into_iter()
                .map(|mut url| {
                    if url.name == "" {
                        url.name = Cow::Owned(String::from(&url.url[..]));

                        url
                    } else {
                        url
                    }
                })
                .collect(),
            ..Default::default()
        }
    }

    fn new_config_with_single_url(mut urls: Vec<Url<'a>>) -> Self {
        let url = urls.get_mut(0).map(std::mem::take).unwrap();
        let primary_name = if url.primary { Some(url.name.to_string()) } else { None };

        Self {
            urls_primary_name: primary_name,
            url: if url.name == "" {
                Some(url.url.to_string())
            } else {
                None
            },
            urls: if url.name != "" { vec![url] } else { Vec::new() },
            ..Default::default()
        }
    }

    /// Constructs a new [`Config`] from [`Iterator`] of [`Url`]s.
    ///
    /// [`Url`]s provided to the [`Config`] will only change the urls Swagger UI is going to use to
    /// fetch the API document.
    ///
    /// # Examples
    /// Create new config with 2 api doc urls.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi1.json", "/api-docs/openapi2.json"]);
    /// ```
    pub fn new<I: IntoIterator<Item = U>, U: Into<Url<'a>>>(urls: I) -> Self {
        Self::new_(urls, None)
    }

    /// Constructs a new [`Config`] from [`Iterator`] of [`Url`]s.
    ///
    /// # Examples
    /// Create new config with oauth config.
    /// ```rust
    /// # use salvo_oapi::swagger::{Config, oauth};
    /// let config = Config::with_oauth_config(
    ///     ["/api-docs/openapi1.json", "/api-docs/openapi2.json"],
    ///     oauth::Config::new(),
    /// );
    /// ```
    pub fn with_oauth_config<I: IntoIterator<Item = U>, U: Into<Url<'a>>>(
        urls: I,
        oauth_config: oauth::Config,
    ) -> Self {
        Self::new_(urls, Some(oauth_config))
    }

    /// Add url to fetch external configuration from.
    ///
    /// # Examples
    ///
    /// Set external config url.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .config_url("http://url.to.external.config");
    /// ```
    pub fn config_url<S: Into<String>>(mut self, config_url: S) -> Self {
        self.config_url = Some(config_url.into());

        self
    }

    /// Add id of the DOM element where `Swagger UI` will put it's user interface.
    ///
    /// The default value is `#swagger-ui`.
    ///
    /// # Examples
    ///
    /// Set custom dom id where the Swagger UI will place it's content.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"]).dom_id("#my-id");
    /// ```
    pub fn dom_id<S: Into<String>>(mut self, dom_id: S) -> Self {
        self.dom_id = Some(dom_id.into());

        self
    }

    /// Set `query_config_enabled` to allow overriding configuration parameters via url `query`
    /// parameters.
    ///
    /// Default value is `false`.
    ///
    /// # Examples
    ///
    /// Enable query config.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .query_config_enabled(true);
    /// ```
    pub fn query_config_enabled(mut self, query_config_enabled: bool) -> Self {
        self.query_config_enabled = Some(query_config_enabled);

        self
    }

    /// Set `deep_linking` to allow deep linking tags and operations.
    ///
    /// Deep linking will automatically scroll to and expand operation when Swagger UI is
    /// given corresponding url fragment. See more at
    /// [deep linking docs](https://github.com/swagger-api/swagger-ui/blob/master/docs/usage/deep-linking.md).
    ///
    /// Deep linking is enabled by default.
    ///
    /// # Examples
    ///
    /// Disable the deep linking.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .deep_linking(false);
    /// ```
    pub fn deep_linking(mut self, deep_linking: bool) -> Self {
        self.deep_linking = Some(deep_linking);

        self
    }

    /// Set `display_operation_id` to `true` to show operation id in the operations list.
    ///
    /// Default value is `false`.
    ///
    /// # Examples
    ///
    /// Allow operation id to be shown.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .display_operation_id(true);
    /// ```
    pub fn display_operation_id(mut self, display_operation_id: bool) -> Self {
        self.display_operation_id = Some(display_operation_id);

        self
    }

    /// Set 'layout' to 'BaseLayout' to only use the base swagger layout without a search header.
    ///
    /// Default value is 'StandaloneLayout'.
    ///
    /// # Examples
    ///
    /// Configure Swagger to use Base Layout instead of Standalone
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .use_base_layout();
    /// ```
    pub fn use_base_layout(mut self) -> Self {
        self.layout = SWAGGER_BASE_LAYOUT;

        self
    }

    /// Add default models expansion depth.
    ///
    /// Setting this to `-1` will completely hide the models.
    ///
    /// # Examples
    ///
    /// Hide all the models.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .default_models_expand_depth(-1);
    /// ```
    pub fn default_models_expand_depth(mut self, default_models_expand_depth: isize) -> Self {
        self.default_models_expand_depth = Some(default_models_expand_depth);

        self
    }

    /// Add default model expansion depth for model on the example section.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .default_model_expand_depth(1);
    /// ```
    pub fn default_model_expand_depth(mut self, default_model_expand_depth: isize) -> Self {
        self.default_model_expand_depth = Some(default_model_expand_depth);

        self
    }

    /// Add `default_model_rendering` to set how models is show when API is first rendered.
    ///
    /// The user can always switch the rendering for given model by clicking the `Model` and `Example Value` links.
    ///
    /// * `example` Makes example rendered first by default.
    /// * `model` Makes model rendered first by default.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .default_model_rendering(r#"["example"*, "model"]"#);
    /// ```
    pub fn default_model_rendering<S: Into<String>>(mut self, default_model_rendering: S) -> Self {
        self.default_model_rendering = Some(default_model_rendering.into());

        self
    }

    /// Set to `true` to show request duration of _**'Try it out'**_ requests _**(in milliseconds)**_.
    ///
    /// Default value is `false`.
    ///
    /// # Examples
    /// Enable request duration of the _**'Try it out'**_ requests.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .display_request_duration(true);
    /// ```
    pub fn display_request_duration(mut self, display_request_duration: bool) -> Self {
        self.display_request_duration = Some(display_request_duration);

        self
    }

    /// Add `doc_expansion` to control default expansion for operations and tags.
    ///
    /// * `list` Will expand only tags.
    /// * `full` Will expand tags and operations.
    /// * `none` Will expand nothing.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .doc_expansion(r#"["list"*, "full", "none"]"#);
    /// ```
    pub fn doc_expansion<S: Into<String>>(mut self, doc_expansion: S) -> Self {
        self.doc_expansion = Some(doc_expansion.into());

        self
    }

    /// Add `filter` to allow filtering of tagged operations.
    ///
    /// When enabled top bar will show and edit box that can be used to filter visible tagged operations.
    /// Filter behaves case sensitive manner and matches anywhere inside the tag.
    ///
    /// Default value is `false`.
    ///
    /// # Examples
    ///
    /// Enable filtering.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .filter(true);
    /// ```
    pub fn filter(mut self, filter: bool) -> Self {
        self.filter = Some(filter);

        self
    }

    /// Add `max_displayed_tags` to restrict shown tagged operations.
    ///
    /// By default all operations are shown.
    ///
    /// # Examples
    ///
    /// Display only 4 operations.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .max_displayed_tags(4);
    /// ```
    pub fn max_displayed_tags(mut self, max_displayed_tags: usize) -> Self {
        self.max_displayed_tags = Some(max_displayed_tags);

        self
    }

    /// Set `show_extensions` to adjust whether vendor extension _**`(x-)`**_ fields and values
    /// are shown for operations, parameters, responses and schemas.
    ///
    /// # Example
    ///
    /// Show vendor extensions.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .show_extensions(true);
    /// ```
    pub fn show_extensions(mut self, show_extensions: bool) -> Self {
        self.show_extensions = Some(show_extensions);

        self
    }

    /// Add `show_common_extensions` to define whether common extension
    /// _**`(pattern, maxLength, minLength, maximum, minimum)`**_ fields and values are shown
    /// for parameters.
    ///
    /// # Examples
    ///
    /// Show common extensions.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .show_common_extensions(true);
    /// ```
    pub fn show_common_extensions(mut self, show_common_extensions: bool) -> Self {
        self.show_common_extensions = Some(show_common_extensions);

        self
    }

    /// Add `try_it_out_enabled` to enable _**'Try it out'**_ section by default.
    ///
    /// Default value is `false`.
    ///
    /// # Examples
    ///
    /// Enable _**'Try it out'**_ section by default.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .try_it_out_enabled(true);
    /// ```
    pub fn try_it_out_enabled(mut self, try_it_out_enabled: bool) -> Self {
        self.try_it_out_enabled = Some(try_it_out_enabled);

        self
    }

    /// Set `request_snippets_enabled` to enable request snippets section.
    ///
    /// If disabled legacy curl snipped will be used.
    ///
    /// Default value is `false`.
    ///
    /// # Examples
    ///
    /// Enable request snippets section.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .request_snippets_enabled(true);
    /// ```
    pub fn request_snippets_enabled(mut self, request_snippets_enabled: bool) -> Self {
        self.request_snippets_enabled = Some(request_snippets_enabled);

        self
    }

    /// Add oauth redirect url.
    ///
    /// # Examples
    ///
    /// Add oauth redirect url.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .oauth2_redirect_url("http://my.oauth2.redirect.url");
    /// ```
    pub fn oauth2_redirect_url<S: Into<String>>(mut self, oauth2_redirect_url: S) -> Self {
        self.oauth2_redirect_url = Some(oauth2_redirect_url.into());

        self
    }

    /// Add `show_mutated_request` to use request returned from `requestInterceptor`
    /// to produce curl command in the UI. If set to `false` the request before `requestInterceptor`
    /// was applied will be used.
    ///
    /// # Examples
    ///
    /// Use request after `requestInterceptor` to produce the curl command.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .show_mutated_request(true);
    /// ```
    pub fn show_mutated_request(mut self, show_mutated_request: bool) -> Self {
        self.show_mutated_request = Some(show_mutated_request);

        self
    }

    /// Add supported http methods for _**'Try it out'**_ operation.
    ///
    /// _**'Try it out'**_ will be enabled based on the given list of http methods when
    /// the operation's http method is included within the list.
    /// By giving an empty list will disable _**'Try it out'**_ from all operations but it will
    /// **not** filter operations from the UI.
    ///
    /// By default all http operations are enabled.
    ///
    /// # Examples
    ///
    /// Set allowed http methods explicitly.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .supported_submit_methods(["get", "put", "post", "delete", "options", "head", "patch", "trace"]);
    /// ```
    ///
    /// Allow _**'Try it out'**_ for only GET operations.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .supported_submit_methods(["get"]);
    /// ```
    pub fn supported_submit_methods<I: IntoIterator<Item = S>, S: Into<String>>(
        mut self,
        supported_submit_methods: I,
    ) -> Self {
        self.supported_submit_methods = Some(
            supported_submit_methods
                .into_iter()
                .map(|method| method.into())
                .collect(),
        );

        self
    }

    /// Add validator url which is used to validate the Swagger spec.
    ///
    /// This can also be set to use locally deployed validator for example see
    /// [Validator Badge](https://github.com/swagger-api/validator-badge) for more details.
    ///
    /// By default swagger.io's online validator _**`(https://validator.swagger.io/validator)`**_ will be used.
    /// Setting this to `none` will disable the validator.
    ///
    /// # Examples
    ///
    /// Disable the validator.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .validator_url("none");
    /// ```
    pub fn validator_url<S: Into<String>>(mut self, validator_url: S) -> Self {
        self.validator_url = Some(validator_url.into());

        self
    }

    /// Set `with_credentials` to enable passing credentials to CORS requests send by browser as defined
    /// [fetch standards](https://fetch.spec.whatwg.org/#credentials).
    ///
    /// **Note!** that Swagger UI cannot currently set cookies cross-domain
    /// (see [swagger-js#1163](https://github.com/swagger-api/swagger-js/issues/1163)) -
    /// as a result, you will have to rely on browser-supplied cookies (which this setting enables sending)
    /// that Swagger UI cannot control.
    ///
    /// # Examples
    ///
    /// Enable passing credentials to CORS requests.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .with_credentials(true);
    /// ```
    pub fn with_credentials(mut self, with_credentials: bool) -> Self {
        self.with_credentials = Some(with_credentials);

        self
    }

    /// Set to `true` to enable authorizations to be persisted throughout browser refresh and close.
    ///
    /// Default value is `false`.
    ///
    ///
    /// # Examples
    ///
    /// Persists authorization throughout browser close and refresh.
    /// ```rust
    /// # use salvo_oapi::swagger::Config;
    /// let config = Config::new(["/api-docs/openapi.json"])
    ///     .persist_authorization(true);
    /// ```
    pub fn persist_authorization(mut self, persist_authorization: bool) -> Self {
        self.persist_authorization = Some(persist_authorization);

        self
    }
}

impl Default for Config<'_> {
    fn default() -> Self {
        Self {
            config_url: Default::default(),
            dom_id: Some("#swagger-ui".to_string()),
            url: Default::default(),
            urls_primary_name: Default::default(),
            urls: Default::default(),
            query_config_enabled: Default::default(),
            deep_linking: Some(true),
            display_operation_id: Default::default(),
            default_models_expand_depth: Default::default(),
            default_model_expand_depth: Default::default(),
            default_model_rendering: Default::default(),
            display_request_duration: Default::default(),
            doc_expansion: Default::default(),
            filter: Default::default(),
            max_displayed_tags: Default::default(),
            show_extensions: Default::default(),
            show_common_extensions: Default::default(),
            try_it_out_enabled: Default::default(),
            request_snippets_enabled: Default::default(),
            oauth2_redirect_url: Default::default(),
            show_mutated_request: Default::default(),
            supported_submit_methods: Default::default(),
            validator_url: Default::default(),
            with_credentials: Default::default(),
            persist_authorization: Default::default(),
            oauth: Default::default(),
            layout: SWAGGER_STANDALONE_LAYOUT,
        }
    }
}

impl<'a> From<&'a str> for Config<'a> {
    fn from(s: &'a str) -> Self {
        Self::new([s])
    }
}

impl From<String> for Config<'_> {
    fn from(s: String) -> Self {
        Self::new([s])
    }
}
