//! Implements [OpenAPI Path Object][paths] types.
//!
//! [paths]: https://spec.openapis.org/oas/latest.html#paths-object
use std::collections::BTreeMap;
use std::iter;
use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};

use super::{Operation, Operations, Parameter, Parameters, PathMap, Server, Servers};

/// Implements [OpenAPI Path Object][paths] types.
///
/// [paths]: https://spec.openapis.org/oas/latest.html#paths-object
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
pub struct Paths(PathMap<String, PathItem>);
impl Deref for Paths {
    type Target = PathMap<String, PathItem>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Paths {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Paths {
    /// Construct a new empty [`Paths`]. This is effectively same as calling [`Paths::default`].
    pub fn new() -> Self {
        Default::default()
    }
    /// Inserts a key-value pair into the instance and returns `self`.
    pub fn path<K: Into<String>, V: Into<PathItem>>(mut self, key: K, value: V) -> Self {
        self.insert(key, value);
        self
    }
    /// Inserts a key-value pair into the instance.
    pub fn insert<K: Into<String>, V: Into<PathItem>>(&mut self, key: K, value: V) {
        let key = key.into();
        let mut value = value.into();
        self.0
            .entry(key)
            .and_modify(|item| {
                if value.summary.is_some() {
                    item.summary = value.summary.take();
                }
                if value.description.is_some() {
                    item.description = value.description.take();
                }
                item.servers.append(&mut value.servers);
                item.parameters.append(&mut value.parameters);
                item.operations.append(&mut value.operations);
            })
            .or_insert(value);
    }
    /// Moves all elements from `other` into `self`, leaving `other` empty.
    ///
    /// If a key from `other` is already present in `self`, the respective
    /// value from `self` will be overwritten with the respective value from `other`.
    pub fn append(&mut self, other: &mut Paths) {
        let items = std::mem::take(&mut other.0);
        for item in items {
            self.insert(item.0, item.1);
        }
    }
    /// Extends a collection with the contents of an iterator.
    pub fn extend<I, K, V>(&mut self, iter: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<PathItem>,
    {
        for (k, v) in iter.into_iter() {
            self.insert(k, v);
        }
    }
}

/// Implements [OpenAPI Path Item Object][path_item] what describes [`Operation`]s available on
/// a single path.
///
/// [path_item]: https://spec.openapis.org/oas/latest.html#path-item-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PathItem {
    /// Optional summary intended to apply all operations in this [`PathItem`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// Optional description intended to apply all operations in this [`PathItem`].
    /// Description supports markdown syntax.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Alternative [`Server`] array to serve all [`Operation`]s in this [`PathItem`] overriding
    /// the global server array.
    #[serde(skip_serializing_if = "Servers::is_empty")]
    pub servers: Servers,

    /// List of [`Parameter`]s common to all [`Operation`]s in this [`PathItem`]. Parameters cannot
    /// contain duplicate parameters. They can be overridden in [`Operation`] level but cannot be
    /// removed there.
    #[serde(skip_serializing_if = "Parameters::is_empty")]
    #[serde(flatten)]
    pub parameters: Parameters,

    /// Map of operations in this [`PathItem`]. Operations can hold only one operation
    /// per [`PathItemType`].
    #[serde(flatten)]
    pub operations: Operations,
}

impl PathItem {
    /// Construct a new [`PathItem`] with provided [`Operation`] mapped to given [`PathItemType`].
    pub fn new<O: Into<Operation>>(path_item_type: PathItemType, operation: O) -> Self {
        let operations = BTreeMap::from_iter(iter::once((path_item_type, operation.into())));

        Self {
            operations: Operations(operations),
            ..Default::default()
        }
    }
    /// Moves all elements from `other` into `self`, leaving `other` empty.
    ///
    /// If a key from `other` is already present in `self`, the respective
    /// value from `self` will be overwritten with the respective value from `other`.
    pub fn append(&mut self, other: &mut Self) {
        self.operations.append(&mut other.operations);
        self.servers.append(&mut other.servers);
        self.parameters.append(&mut other.parameters);
        if other.description.is_some() {
            self.description = other.description.take();
        }
        if other.summary.is_some() {
            self.summary = other.summary.take();
        }
    }

    /// Append a new [`Operation`] by [`PathItemType`] to this [`PathItem`]. Operations can
    /// hold only one operation per [`PathItemType`].
    pub fn add_operation<O: Into<Operation>>(
        mut self,
        path_item_type: PathItemType,
        operation: O,
    ) -> Self {
        self.operations.insert(path_item_type, operation.into());
        self
    }

    /// Add or change summary intended to apply all operations in this [`PathItem`].
    pub fn summary<S: Into<String>>(mut self, summary: S) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Add or change optional description intended to apply all operations in this [`PathItem`].
    /// Description supports markdown syntax.
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add list of alternative [`Server`]s to serve all [`Operation`]s in this [`PathItem`] overriding
    /// the global server array.
    pub fn servers<I: IntoIterator<Item = Server>>(mut self, servers: I) -> Self {
        self.servers = Servers(servers.into_iter().collect());
        self
    }

    /// Append list of [`Parameter`]s common to all [`Operation`]s to this [`PathItem`].
    pub fn parameters<I: IntoIterator<Item = Parameter>>(mut self, parameters: I) -> Self {
        self.parameters = Parameters(parameters.into_iter().collect());
        self
    }
}

/// Path item operation type.
#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum PathItemType {
    /// Type mapping for HTTP _GET_ request.
    Get,
    /// Type mapping for HTTP _POST_ request.
    Post,
    /// Type mapping for HTTP _PUT_ request.
    Put,
    /// Type mapping for HTTP _DELETE_ request.
    Delete,
    /// Type mapping for HTTP _OPTIONS_ request.
    Options,
    /// Type mapping for HTTP _HEAD_ request.
    Head,
    /// Type mapping for HTTP _PATCH_ request.
    Patch,
    /// Type mapping for HTTP _TRACE_ request.
    Trace,
    /// Type mapping for HTTP _CONNECT_ request.
    Connect,
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;
    use crate::oapi::response::Response;

    #[test]
    fn test_build_path_item() {
        let path_item = PathItem::new(PathItemType::Get, Operation::new())
            .summary("summary")
            .description("description")
            .servers(Servers::new())
            .parameters(Parameters::new());

        assert_json_eq!(
            path_item,
            json!({
                "description": "description",
                "summary": "summary",
                "get": {
                    "responses": {}
                }
            })
        )
    }

    #[test]
    fn test_path_item_append() {
        let mut path_item = PathItem::new(
            PathItemType::Get,
            Operation::new().add_response("200", Response::new("Get success")),
        );
        let mut other_path_item = PathItem::new(
            PathItemType::Post,
            Operation::new().add_response("200", Response::new("Post success")),
        )
        .description("description")
        .summary("summary");
        path_item.append(&mut other_path_item);

        assert_json_eq!(
            path_item,
            json!({
                "description": "description",
                "summary": "summary",
                "get": {
                    "responses": {
                        "200": {
                            "description": "Get success"
                        }
                    }
                },
                "post": {
                    "responses": {
                        "200": {
                            "description": "Post success"
                        }
                    }
                }
            })
        )
    }

    #[test]
    fn test_path_item_add_operation() {
        let path_item = PathItem::new(
            PathItemType::Get,
            Operation::new().add_response("200", Response::new("Get success")),
        )
        .add_operation(
            PathItemType::Post,
            Operation::new().add_response("200", Response::new("Post success")),
        );

        assert_json_eq!(
            path_item,
            json!({
                "get": {
                    "responses": {
                        "200": {
                            "description": "Get success"
                        }
                    }
                },
                "post": {
                    "responses": {
                        "200": {
                            "description": "Post success"
                        }
                    }
                }
            })
        )
    }

    #[test]
    fn test_paths_extend() {
        let mut paths = Paths::new().path(
            "/api/do_something",
            PathItem::new(
                PathItemType::Get,
                Operation::new().add_response("200", Response::new("Get success")),
            ),
        );
        paths.extend([(
            "/api/do_something",
            PathItem::new(
                PathItemType::Post,
                Operation::new().add_response("200", Response::new("Post success")),
            )
            .summary("summary")
            .description("description"),
        )]);

        assert_json_eq!(
            paths,
            json!({
                "/api/do_something": {
                    "description": "description",
                    "summary": "summary",
                    "get": {
                        "responses": {
                            "200": {
                                "description": "Get success"
                            }
                        }
                    },
                    "post": {
                        "responses": {
                            "200": {
                                "description": "Post success"
                            }
                        }
                    }
                }
            })
        );
    }

    #[test]
    fn test_paths_deref() {
        let paths = Paths::new();
        assert_eq!(0, paths.len());
    }
}
