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
//! # Examples
//!
//! Create new server with relative path.
//! ```rust
//! # use salvo_oapi::server::Server;
//! Server::new("/api/v1");
//! ```
//!
//! Create server with custom url using a builder.
//! ```rust
//! # use salvo_oapi::server::Server;
//! Server::new("https://alternative.api.url.test/api");
//! ```
//!
//! Create server with builder and variable substitution.
//! ```rust
//! # use salvo_oapi::server::{Server, ServerVariable};
//! Server::new("/api/{version}/{username}")
//!     .add_variable("version", ServerVariable::new()
//!         .enum_values(["v1", "v2"])
//!         .default_value("v1"))
//!     .add_variable("username", ServerVariable::new()
//!         .default_value("the_user"));
//! ```
//!
//! [server]: https://spec.openapis.org/oas/latest.html#server-object
//! [openapi]: ../struct.OpenApi.html
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]

/// Collection for [`Server`] objects.
pub struct Servers(pub BTreeSet<Server>);
impl Deref for Servers {
    type Target = BTreeSet<Server>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Servers {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl IntoIterator for Servers {
    type Item = Server;
    type IntoIter = <BTreeSet<Server> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
impl Servers {
    /// Construct a new empty [`Servers`]. This is effectively same as calling [`Servers::default`].
    pub fn new() -> Self {
        Default::default()
    }
    /// Returns `true` if instance contains no elements.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Inserts a server into the instance and returns `self`.
    pub fn server<S: Into<Server>>(mut self, server: S) -> Self {
        self.insert(server);
        self
    }
    /// Inserts a server into the instance.
    pub fn insert<S: Into<Server>>(&mut self, server: S) {
        let server = server.into();
        let exist_server = self.0.iter().find(|s| s.url == server.url).cloned();
        if let Some(mut exist_server) = exist_server {
            let Server {
                description,
                mut variables,
                ..
            } = server;
            exist_server.variables.append(&mut variables);
            if description.is_some() {
                exist_server.description = description;
            }
            self.0.remove(&exist_server);
            self.0.insert(exist_server);
        } else {
            self.0.insert(server);
        }
    }

    /// Moves all elements from `other` into `self`, leaving `other` empty.
    ///
    /// If a key from `other` is already present in `self`, the respective
    /// value from `self` will be overwritten with the respective value from `other`.
    pub fn append(&mut self, other: &mut Servers) {
        let servers = std::mem::take(&mut other.0);
        for server in servers {
            self.insert(server);
        }
    }
    /// Extends a collection with the contents of an iterator.
    pub fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = Server>,
    {
        for server in iter.into_iter() {
            self.insert(server);
        }
    }
}

/// Represents target server object. It can be used to alter server connection for
/// _**path operations**_.
///
/// By default OpenAPI will implicitly implement [`Server`] with `url = "/"` if no servers is provided to
/// the [`OpenApi`][openapi].
///
/// [openapi]: ../struct.OpenApi.html
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
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
    #[serde(skip_serializing_if = "ServerVariables::is_empty")]
    pub variables: ServerVariables,
}

impl Ord for Server {
    fn cmp(&self, other: &Self) -> Ordering {
        self.url.cmp(&other.url)
    }
}
impl PartialOrd for Server {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
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
    /// # use salvo_oapi::server::Server;
    ///  Server::new("/api/v1");
    /// ```
    ///
    /// Create new server with alternative server.
    /// ```
    /// # use salvo_oapi::server::Server;
    ///  Server::new("https://alternative.pet-api.test/api/v1");
    /// ```
    pub fn new<S: Into<String>>(url: S) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }
    /// Add url to the target [`Server`].
    pub fn url<U: Into<String>>(mut self, url: U) -> Self {
        self.url = url.into();
        self
    }

    /// Add or change description of the [`Server`].
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add parameter to [`Server`] which is used to substitute values in [`Server::url`] and returns `Self`.
    ///
    /// * `name` Defines name of the parameter which is being substituted within the url. If url has
    ///   `{username}` substitution then the name should be `username`.
    /// * `parameter` Use [`ServerVariable`] to define how the parameter is being substituted
    ///   within the url.
    pub fn add_variable<N: Into<String>, V: Into<ServerVariable>>(mut self, name: N, variable: V) -> Self {
        self.variables.insert(name.into(), variable.into());
        self
    }
}

/// Server Variables information for OpenApi.
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Eq, Debug)]
pub struct ServerVariables(pub BTreeMap<String, ServerVariable>);
impl Deref for ServerVariables {
    type Target = BTreeMap<String, ServerVariable>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for ServerVariables {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl ServerVariables {
    /// Construct a new empty [`ServerVariables`]. This is effectively same as calling [`ServerVariables::default`].
    pub fn new() -> Self {
        Default::default()
    }
    /// Returns `true` if instance contains no elements.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Inserts a key-value pair into the instance and returns `self`.
    pub fn server_varible<K: Into<String>, V: Into<ServerVariable>>(mut self, key: K, variable: V) -> Self {
        self.insert(key, variable);
        self
    }
    /// Inserts a key-value pair into the instance.
    pub fn insert<K: Into<String>, V: Into<ServerVariable>>(&mut self, key: K, variable: V) {
        let key = key.into();
        let mut variable = variable.into();
        self.0
            .entry(key)
            .and_modify(|item| {
                if variable.description.is_some() {
                    item.description = variable.description.take();
                }
                item.default_value.clone_from(&variable.default_value);
                item.enum_values.append(&mut variable.enum_values);
            })
            .or_insert(variable);
    }
    /// Moves all elements from `other` into `self`, leaving `other` empty.
    ///
    /// If a key from `other` is already present in `self`, the respective
    /// value from `self` will be overwritten with the respective value from `other`.
    pub fn append(&mut self, other: &mut ServerVariables) {
        let variables = std::mem::take(&mut other.0);
        for (key, variable) in variables {
            self.insert(key, variable);
        }
    }
    /// Extends a collection with the contents of an iterator.
    pub fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = (String, ServerVariable)>,
    {
        for (key, variable) in iter.into_iter() {
            self.insert(key, variable);
        }
    }
}

/// Implements [OpenAPI Server Variable][server_variable] used to substitute variables in [`Server::url`].
///
/// [server_variable]: https://spec.openapis.org/oas/latest.html#server-variable-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct ServerVariable {
    /// Default value used to substitute parameter if no other value is being provided.
    #[serde(rename = "default")]
    default_value: String,

    /// Optional description describing the variable of substitution. Markdown syntax is supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,

    /// Enum values can be used to limit possible options for substitution. If enum values is used
    /// the [`ServerVariable::default_value`] must contain one of the enum values.
    #[serde(rename = "enum", skip_serializing_if = "BTreeSet::is_empty")]
    enum_values: BTreeSet<String>,
}

impl ServerVariable {
    /// Construct a new empty [`ServerVariable`]. This is effectively same as calling [`ServerVariable::default`].
    pub fn new() -> Self {
        Default::default()
    }
    /// Add default value for substitution.
    pub fn default_value<S: Into<String>>(mut self, default_value: S) -> Self {
        self.default_value = default_value.into();
        self
    }

    /// Add or change description of substituted parameter.
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add or change possible values used to substitute parameter.
    pub fn enum_values<I: IntoIterator<Item = V>, V: Into<String>>(mut self, enum_values: I) -> Self {
        self.enum_values = enum_values.into_iter().map(|value| value.into()).collect();
        self
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

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
    Server::new("/api/{version}/{username}")
        .add_variable(
            "version",
            ServerVariable::new()
                .enum_values(["v1", "v2"])
                .description("api version")
                .default_value("v1")
        )
        .add_variable(
            "username",
            ServerVariable::new().default_value("the_user")
        );
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

    #[test]
    fn test_servers_is_empty() {
        let servers = Servers::new();
        assert!(servers.is_empty());
    }

    #[test]
    fn test_servers_server() {
        let servers = Servers::new();
        let server = Server::new("/api/v1").description("api v1");
        let servers = servers.server(server);
        assert!(servers.len() == 1);
    }

    #[test]
    fn test_servers_insert() {
        let mut servers = Servers::new();
        let server = Server::new("/api/v1").description("api v1");
        servers.insert(server);
        assert!(servers.len() == 1);
    }

    #[test]
    fn test_servers_insert_existed_server() {
        let mut servers = Servers::new();
        let server1 = Server::new("/api/v1".to_string())
            .description("api v1")
            .add_variable("key1", ServerVariable::new());
        servers.insert(server1);

        let server2 = Server::new("/api/v1".to_string())
            .description("api v1 new description")
            .add_variable("key2", ServerVariable::new());
        servers.insert(server2);

        assert!(servers.len() == 1);
        assert_json_eq!(
            servers,
            json!([
                {
                    "description": "api v1 new description",
                    "url": "/api/v1",
                    "variables": {
                        "key1": {
                            "default": ""
                        },
                        "key2": {
                            "default": ""
                        }
                    }
                }
            ])
        )
    }

    #[test]
    fn test_servers_append() {
        let mut servers = Servers::new();

        let server = Server::new("/api/v1").description("api v1");
        let mut other_servers: Servers = Servers::new();

        other_servers.insert(server);
        assert!(!other_servers.is_empty());

        servers.append(&mut other_servers);
        assert!(!servers.is_empty());
    }

    #[test]
    fn test_servers_extend() {
        let mut servers = Servers::new();

        let server = Server::new("/api/v1").description("api v1");
        let mut other_servers: Servers = Servers::new();

        other_servers.insert(server);
        assert!(!other_servers.is_empty());

        servers.extend(other_servers);
        assert!(!servers.is_empty());
    }

    #[test]
    fn test_servers_deref() {
        let mut servers = Servers::new();
        let server = Server::new("/api/v1").description("api v1");
        servers.insert(server);
        assert!(servers.len() == 1);
        assert!(servers.deref().len() == 1);

        servers.deref_mut().clear();
        assert!(servers.is_empty());
    }

    #[test]
    fn test_server_set_url() {
        let server = Server::new("/api/v1");
        assert_eq!(server.url, "/api/v1");

        let server = server.url("/new/api/v1");
        assert_eq!(server.url, "/new/api/v1");
    }

    #[test]
    fn test_server_cmp() {
        let server_a = Server::new("/api/v1");
        let server_b = Server::new("/api/v2");
        assert!(server_a < server_b);
    }

    #[test]
    fn test_server_variables_is_empty() {
        let server_variables = ServerVariables::new();
        assert!(server_variables.is_empty());
    }

    #[test]
    fn test_server_variables_server_varible() {
        let server_variables = ServerVariables::new();
        let variable = ServerVariable::new();
        let server_variables = server_variables.server_varible("key", variable);

        assert!(!server_variables.is_empty());
    }

    #[test]
    fn test_server_variables_insert() {
        let mut server_variables = ServerVariables::new();
        let variable = ServerVariable::new();
        server_variables.insert("key", variable);
        assert!(server_variables.len() == 1);

        let new_variable = ServerVariable::new().description("description");
        server_variables.insert("key", new_variable);
        assert!(server_variables.len() == 1);
    }

    #[test]
    fn test_server_variables_append() {
        let mut server_variables = ServerVariables::new();

        let mut other_server_variables = ServerVariables::new();
        let variable = ServerVariable::new();
        other_server_variables.insert("key", variable);

        server_variables.append(&mut other_server_variables);
        assert!(server_variables.len() == 1);
    }

    #[test]
    fn test_server_variables_extend() {
        let mut server_variables = ServerVariables::new();

        let mut other_server_variables = ServerVariables::new();
        let variable = ServerVariable::new();
        other_server_variables.insert("key", variable);

        server_variables.extend(other_server_variables.0);
        assert!(server_variables.len() == 1);
    }

    #[test]
    fn test_server_variables_deref() {
        let mut server_variables = ServerVariables::new();

        let variable = ServerVariable::new().default_value("default_value");
        server_variables.insert("key", variable);

        assert!(!server_variables.is_empty());
        assert!(server_variables.deref().len() == 1);

        server_variables.deref_mut().clear();
        assert!(server_variables.is_empty());
    }
}
