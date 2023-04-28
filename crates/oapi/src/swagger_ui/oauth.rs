//! Implements Swagger UI [oauth configuration](https://github.com/swagger-api/swagger-ui/blob/master/docs/usage/oauth2.md) options.

use std::collections::HashMap;

use serde::Serialize;

const END_MARKER: &str = "//</editor-fold>";

/// Object used to alter Swagger UI oauth settings.
///
/// # Examples
///
/// ```
/// # use salvo_oapi::swagger::oauth;
/// let config = oauth::Config::new()
///     .client_id("client-id")
///     .use_pkce_with_authorization_code_grant(true);
/// ```
#[non_exhaustive]
#[derive(Default, Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// oauth client_id the Swagger UI is using for auth flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// oauth client_secret the Swagger UI is using for auth flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,

    /// oauth realm the Swagger UI is using for auth flow.
    /// realm query parameter (for oauth1) added to authorizationUrl and tokenUrl.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realm: Option<String>,

    /// oauth app_name the Swagger UI is using for auth flow.
    /// application name, displayed in authorization popup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,

    /// oauth scope_separator the Swagger UI is using for auth flow.
    /// scope separator for passing scopes, encoded before calling, default value is a space (encoded value %20).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_separator: Option<String>,

    /// oauth scopes the Swagger UI is using for auth flow.
    /// [`Vec<String>`] of initially selected oauth scopes, default is empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,

    /// oauth additional_query_string_params the Swagger UI is using for auth flow.
    /// [`HashMap<String, String>`] of additional query parameters added to authorizationUrl and tokenUrl
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_query_string_params: Option<HashMap<String, String>>,

    /// oauth use_basic_authentication_with_access_code_grant the Swagger UI is using for auth flow.
    /// Only activated for the accessCode flow. During the authorization_code request to the tokenUrl,
    /// pass the [Client Password](https://tools.ietf.org/html/rfc6749#section-2.3.1) using the HTTP Basic Authentication scheme
    /// (Authorization header with Basic base64encode(client_id + client_secret)).
    /// The default is false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_basic_authentication_with_access_code_grant: Option<bool>,

    /// oauth use_pkce_with_authorization_code_grant the Swagger UI is using for auth flow.
    /// Only applies to authorizatonCode flows. [Proof Key for Code Exchange](https://tools.ietf.org/html/rfc7636)
    /// brings enhanced security for OAuth public clients.
    /// The default is false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_pkce_with_authorization_code_grant: Option<bool>,
}

impl Config {
    /// Create a new [`Config`] for oauth auth flow.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_oapi::swagger::oauth;
    /// let config = oauth::Config::new();
    /// ```
    pub fn new() -> Self {
        Self { ..Default::default() }
    }

    /// Add client_id into [`Config`].
    ///
    /// Method takes one argument which exposes the client_id to the user.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_oapi::swagger::oauth;
    /// let config = oauth::Config::new()
    ///     .client_id("client-id");
    /// ```
    pub fn client_id(mut self, client_id: &str) -> Self {
        self.client_id = Some(String::from(client_id));

        self
    }

    /// Add client_secret into [`Config`].
    ///
    /// Method takes one argument which exposes the client_secret to the user.
    /// 🚨 Never use this parameter in your production environment.
    /// It exposes crucial security information. This feature is intended for dev/test environments only. 🚨
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_oapi::swagger::oauth;
    /// let config = oauth::Config::new()
    ///     .client_secret("client-secret");
    /// ```
    pub fn client_secret(mut self, client_secret: &str) -> Self {
        self.client_secret = Some(String::from(client_secret));

        self
    }

    /// Add realm into [`Config`].
    ///
    /// Method takes one argument which exposes the realm to the user.
    /// realm query parameter (for oauth1) added to authorizationUrl and tokenUrl.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_oapi::swagger::oauth;
    /// let config = oauth::Config::new()
    ///     .realm("realm");
    /// ```
    pub fn realm(mut self, realm: &str) -> Self {
        self.realm = Some(String::from(realm));

        self
    }

    /// Add app_name into [`Config`].
    ///
    /// Method takes one argument which exposes the app_name to the user.
    /// application name, displayed in authorization popup.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_oapi::swagger::oauth;
    /// let config = oauth::Config::new()
    ///     .app_name("app-name");
    /// ```
    pub fn app_name(mut self, app_name: &str) -> Self {
        self.app_name = Some(String::from(app_name));

        self
    }

    /// Add scope_separator into [`Config`].
    ///
    /// Method takes one argument which exposes the scope_separator to the user.
    /// scope separator for passing scopes, encoded before calling, default value is a space (encoded value %20).
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_oapi::swagger::oauth;
    /// let config = oauth::Config::new()
    ///     .scope_separator(",");
    /// ```
    pub fn scope_separator(mut self, scope_separator: &str) -> Self {
        self.scope_separator = Some(String::from(scope_separator));

        self
    }

    /// Add scopes into [`Config`].
    ///
    /// Method takes one argument which exposes the scopes to the user.
    /// [`Vec<String>`] of initially selected oauth scopes, default is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_oapi::swagger::oauth;
    /// let config = oauth::Config::new()
    ///     .scopes(vec![String::from("openid")]);
    /// ```
    pub fn scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = Some(scopes);

        self
    }

    /// Add additional_query_string_params into [`Config`].
    ///
    /// Method takes one argument which exposes the additional_query_string_params to the user.
    /// [`HashMap<String, String>`] of additional query parameters added to authorizationUrl and tokenUrl
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_oapi::swagger::oauth;
    /// # use std::collections::HashMap;
    /// let config = oauth::Config::new()
    ///     .additional_query_string_params(HashMap::from([(String::from("a"), String::from("1"))]));
    /// ```
    pub fn additional_query_string_params(mut self, additional_query_string_params: HashMap<String, String>) -> Self {
        self.additional_query_string_params = Some(additional_query_string_params);

        self
    }

    /// Add use_basic_authentication_with_access_code_grant into [`Config`].
    ///
    /// Method takes one argument which exposes the use_basic_authentication_with_access_code_grant to the user.
    /// Only activated for the accessCode flow. During the authorization_code request to the tokenUrl,
    /// pass the [Client Password](https://tools.ietf.org/html/rfc6749#section-2.3.1) using the HTTP Basic Authentication scheme
    /// (Authorization header with Basic base64encode(client_id + client_secret)).
    /// The default is false
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_oapi::swagger::oauth;
    /// let config = oauth::Config::new()
    ///     .use_basic_authentication_with_access_code_grant(true);
    /// ```
    pub fn use_basic_authentication_with_access_code_grant(
        mut self,
        use_basic_authentication_with_access_code_grant: bool,
    ) -> Self {
        self.use_basic_authentication_with_access_code_grant = Some(use_basic_authentication_with_access_code_grant);

        self
    }

    /// Add use_pkce_with_authorization_code_grant into [`Config`].
    ///
    /// Method takes one argument which exposes the use_pkce_with_authorization_code_grant to the user.
    /// Only applies to authorizatonCode flows. [Proof Key for Code Exchange](https://tools.ietf.org/html/rfc7636)
    /// brings enhanced security for OAuth public clients.
    /// The default is false
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_oapi::swagger::oauth;
    /// let config = oauth::Config::new()
    ///     .use_pkce_with_authorization_code_grant(true);
    /// ```
    pub fn use_pkce_with_authorization_code_grant(mut self, use_pkce_with_authorization_code_grant: bool) -> Self {
        self.use_pkce_with_authorization_code_grant = Some(use_pkce_with_authorization_code_grant);

        self
    }
}

pub(crate) fn format_swagger_config(config: &Config, file: String) -> serde_json::Result<String> {
    let init_string = format!(
        "{}\nui.initOAuth({});",
        END_MARKER,
        serde_json::to_string_pretty(config)?
    );
    Ok(file.replace(END_MARKER, &init_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_CONTENT: &str = r###""
    //<editor-fold desc=\"Changeable Configuration Block\">
    window.ui = SwaggerUIBundle({
        {{urls}},
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
    });
    //</editor-fold>
    ""###;

    #[test]
    fn format_swagger_config_oauth() {
        let config = Config {
            client_id: Some(String::from("my-special-client")),
            ..Default::default()
        };
        let file = super::format_swagger_config(&config, TEST_CONTENT.to_string()).unwrap();

        let expected = r#"
ui.initOAuth({
  "clientId": "my-special-client"
});"#;
        assert!(
            file.contains(expected),
            "expected file to contain {}, was {}",
            expected,
            file
        )
    }
}
