//! Implements [OpenAPI Server Object][server] types to configure target servers.
//!
//! OpenAPI will implicitly add [`Server`] with `url = "/"` to [`OpenApi`][openapi] when no servers
//! are defined.
//!
//! [`Server`] can be used to alter connection url for _**path operations**_. It can be a
//! relative path e.g `/api/v1` or valid http url e.g. `http://alternative.api.com/api/v1`.
//!
//! Relative path will append to the **sever address** so the connection url for _**path operations**_
//! will become `server address + relative path`.
//!
//! Optionally it also supports parameter substitution with `{variable}` syntax.
//!
//! See [`Modify`][modify] trait for details how add servers to [`OpenApi`][openapi].
//!
//! # Examples
//!
//! Create new server with relative path.
//! ```rust
//! # use salvo_oapi::openapi::server::Server;
//! Server::new("/api/v1");
//! ```
//!
//! Create server with custom url using a builder.
//! ```rust
//! # use salvo_oapi::openapi::server::ServerBuilder;
//! ServerBuilder::new().url("https://alternative.api.url.test/api").build();
//! ```
//!
//! Create server with builder and variable substitution.
//! ```rust
//! # use salvo_oapi::openapi::server::{ServerBuilder, ServerVariableBuilder};
//! ServerBuilder::new().url("/api/{version}/{username}")
//!     .parameter("version", ServerVariableBuilder::new()
//!         .enum_values(Some(["v1", "v2"]))
//!         .default_value("v1"))
//!     .parameter("username", ServerVariableBuilder::new()
//!         .default_value("the_user")).build();
//! ```
//!
//! [server]: https://spec.openapis.org/oas/latest.html#server-object
//! [openapi]: ../struct.OpenApi.html
//! [modify]: ../../trait.Modify.html
use std::{collections::BTreeMap, iter};

use serde::{Deserialize, Serialize};

use super::{builder, set_value};

builder! {
    ServerBuilder;

    /// Represents target server object. It can be used to alter server connection for
    /// _**path operations**_.
    ///
    /// By default OpenAPI will implicitly implement [`Server`] with `url = "/"` if no servers is provided to
    /// the [`OpenApi`][openapi].
    ///
    /// [openapi]: ../struct.OpenApi.html
    #[non_exhaustive]
    #[derive(Serialize, Deserialize, Default, Clone,Debug, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub struct Server {
        /// Target url of the [`Server`]. It can be valid http url or relative path.
        ///
        /// Url also supports variable substitution with `{variable}` syntax. The substitutions
        /// then can be configured with [`Server::variables`] map.
        pub url: String,

        /// Optional description describing the target server url. Description supports markdown syntax.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,

        /// Optional map of variable name and its substitution value used in [`Server::url`].
        #[serde(skip_serializing_if = "Option::is_none")]
        pub variables: Option<BTreeMap<String, ServerVariable>>,
    }
}

impl Server {
    /// Construct a new [`Server`] with given url. Url can be valid http url or context path of the url.
    ///
    /// If url is valid http url then all path operation request's will be forwarded to the selected [`Server`].
    ///
    /// If url is path of url e.g. `/api/v1` then the url will be appended to the servers address and the
    /// operations will be forwarded to location `server address + url`.
    ///
    ///
    /// # Examples
    ///
    /// Create new server with url path.
    /// ```
    /// # use salvo_oapi::openapi::server::Server;
    ///  Server::new("/api/v1");
    /// ```
    ///
    /// Create new server with alternative server.
    /// ```
    /// # use salvo_oapi::openapi::server::Server;
    ///  Server::new("https://alternative.pet-api.test/api/v1");
    /// ```
    pub fn new<S: Into<String>>(url: S) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }
}

impl ServerBuilder {
    /// Add url to the target [`Server`].
    pub fn url<U: Into<String>>(mut self, url: U) -> Self {
        set_value!(self url url.into())
    }

    /// Add or change description of the [`Server`].
    pub fn description<S: Into<String>>(mut self, description: Option<S>) -> Self {
        set_value!(self description description.map(|description| description.into()))
    }

    /// Add parameter to [`Server`] which is used to substitute values in [`Server::url`].
    ///
    /// * `name` Defines name of the parameter which is being substituted within the url. If url has
    ///   `{username}` substitution then the name should be `username`.
    /// * `parameter` Use [`ServerVariableBuilder`] to define how the parameter is being substituted
    ///   within the url.
    pub fn parameter<N: Into<String>, V: Into<ServerVariable>>(
        mut self,
        name: N,
        variable: V,
    ) -> Self {
        match self.variables {
            Some(ref mut variables) => {
                variables.insert(name.into(), variable.into());
            }
            None => {
                self.variables = Some(BTreeMap::from_iter(iter::once((
                    name.into(),
                    variable.into(),
                ))))
            }
        }

        self
    }
}

builder! {
    ServerVariableBuilder;

    /// Implements [OpenAPI Server Variable][server_variable] used to substitute variables in [`Server::url`].
    ///
    /// [server_variable]: https://spec.openapis.org/oas/latest.html#server-variable-object
    #[non_exhaustive]
    #[derive(Serialize, Deserialize, Default, Clone,Debug, PartialEq, Eq)]
    pub struct ServerVariable {
        /// Default value used to substitute parameter if no other value is being provided.
        #[serde(rename = "default")]
        default_value: String,

        /// Optional description describing the variable of substitution. Markdown syntax is supported.
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,

        /// Enum values can be used to limit possible options for substitution. If enum values is used
        /// the [`ServerVariable::default_value`] must contain one of the enum values.
        #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
        enum_values: Option<Vec<String>>,
    }
}

impl ServerVariableBuilder {
    /// Add default value for substitution.
    pub fn default_value<S: Into<String>>(mut self, default_value: S) -> Self {
        set_value!(self default_value default_value.into())
    }

    /// Add or change description of substituted parameter.
    pub fn description<S: Into<String>>(mut self, description: Option<S>) -> Self {
        set_value!(self description description.map(|description| description.into()))
    }

    /// Add or change possible values used to substitute parameter.
    pub fn enum_values<I: IntoIterator<Item = V>, V: Into<String>>(
        mut self,
        enum_values: Option<I>,
    ) -> Self {
        set_value!(self enum_values enum_values
            .map(|enum_values| enum_values.into_iter().map(|value| value.into()).collect()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test_fn {
        ($name:ident: $schema:expr; $expected:literal) => {
            #[test]
            fn $name() {
                let value = serde_json::to_value($schema).unwrap();
                let expected_value: serde_json::Value = serde_json::from_str($expected).unwrap();

                assert_eq!(
                    value,
                    expected_value,
                    "testing serializing \"{}\": \nactual:\n{}\nexpected:\n{}",
                    stringify!($name),
                    value,
                    expected_value
                );

                println!("{}", &serde_json::to_string_pretty(&$schema).unwrap());
            }
        };
    }

    test_fn! {
    create_server_with_builder_and_variable_substitution:
    ServerBuilder::new().url("/api/{version}/{username}")
        .parameter("version", ServerVariableBuilder::new()
            .enum_values(Some(["v1", "v2"]))
            .description(Some("api version"))
            .default_value("v1"))
        .parameter("username", ServerVariableBuilder::new()
            .default_value("the_user")).build();
    r###"{
  "url": "/api/{version}/{username}",
  "variables": {
      "version": {
          "enum": ["v1", "v2"],
          "default": "v1",
          "description": "api version"
      },
      "username": {
          "default": "the_user"
      }
  }
}"###
    }
}
