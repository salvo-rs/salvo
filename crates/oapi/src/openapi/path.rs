//! Implements [OpenAPI Path Object][paths] types.
//!
//! [paths]: https://spec.openapis.org/oas/latest.html#paths-object
use std::{collections::BTreeMap, iter};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    request_body::RequestBody, set_value, Deprecated, ExternalDocs, RefOr, Required, Schema, SecurityRequirement,
    Server,
};
use crate::{Operation, Parameter};

/// Implements [OpenAPI Paths Object][paths].
///
/// Holds relative paths to matching endpoints and operations. The path is appended to the url
/// from [`Server`] object to construct a full url for endpoint.
///
/// [paths]: https://spec.openapis.org/oas/latest.html#paths-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Paths {
    /// Map of relative paths with [`PathItem`]s holding [`Operation`]s matching
    /// api endpoints.
    pub paths: BTreeMap<String, PathItem>,
}

impl Paths {
    /// Construct a new [`Paths`] object.
    pub fn new() -> Self {
        Default::default()
    }

    /// Return _`Option`_ of reference to [`PathItem`] by given relative path _`P`_ if one exists
    /// in [`Paths::paths`] map. Otherwise will return `None`.
    ///
    /// # Examples
    ///
    /// _**Get user path item.**_
    /// ```
    /// # use salvo_oapi::path::{Paths, PathItemType};
    /// # let paths = Paths::new();
    /// let path_item = paths.get_path_item("/api/v1/user");
    /// ```
    pub fn get_path_item<P: AsRef<str>>(&self, path: P) -> Option<&PathItem> {
        self.paths.get(path.as_ref())
    }

    /// Return _`Option`_ of reference to [`Operation`] from map of paths or `None` if not found.
    ///
    /// * First will try to find [`PathItem`] by given relative path _`P`_ e.g. `"/api/v1/user"`.
    /// * Then tries to find [`Operation`] from [`PathItem`]'s operations by given [`PathItemType`].
    ///
    /// # Examples
    ///
    /// _**Get user operation from paths.**_
    /// ```
    /// # use salvo_oapi::path::{Paths, PathItemType};
    /// # let paths = Paths::new();
    /// let operation = paths.get_path_operation("/api/v1/user", PathItemType::Get);
    /// ```
    pub fn get_path_operation<P: AsRef<str>>(&self, path: P, item_type: PathItemType) -> Option<&Operation> {
        self.paths
            .get(path.as_ref())
            .and_then(|path| path.operations.get(&item_type))
    }

    /// Append [`PathItem`] with path to map of paths. If path already exists it will merge [`Operation`]s of
    /// [`PathItem`] with already found path item operations.
    pub fn path<I: Into<String>>(mut self, path: I, mut item: PathItem) -> Self {
        let path_string = path.into();
        if let Some(existing_item) = self.paths.get_mut(&path_string) {
            existing_item.operations.append(&mut item.operations);
        } else {
            self.paths.insert(path_string, item);
        }

        self
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<Server>>,

    /// List of [`Parameter`]s common to all [`Operation`]s in this [`PathItem`]. Parameters cannot
    /// contain duplicate parameters. They can be overridden in [`Operation`] level but cannot be
    /// removed there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<Parameter>>,

    /// Map of operations in this [`PathItem`]. Operations can hold only one operation
    /// per [`PathItemType`].
    #[serde(flatten)]
    pub operations: BTreeMap<PathItemType, Operation>,
}

impl PathItem {
    /// Construct a new [`PathItem`] with provided [`Operation`] mapped to given [`PathItemType`].
    pub fn new<O: Into<Operation>>(path_item_type: PathItemType, operation: O) -> Self {
        let operations = BTreeMap::from_iter(iter::once((path_item_type, operation.into())));

        Self {
            operations,
            ..Default::default()
        }
    }

    /// Append a new [`Operation`] by [`PathItemType`] to this [`PathItem`]. Operations can
    /// hold only one operation per [`PathItemType`].
    pub fn operation<O: Into<Operation>>(mut self, path_item_type: PathItemType, operation: O) -> Self {
        self.operations.insert(path_item_type, operation.into());

        self
    }

    /// Add or change summary intended to apply all operations in this [`PathItem`].
    pub fn summary<S: Into<String>>(mut self, summary: S) -> Self {
        set_value!(self summary Some(summary.into()))
    }

    /// Add or change optional description intended to apply all operations in this [`PathItem`].
    /// Description supports markdown syntax.
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        set_value!(self description Some(description.into()))
    }

    /// Add list of alternative [`Server`]s to serve all [`Operation`]s in this [`PathItem`] overriding
    /// the global server array.
    pub fn servers<I: IntoIterator<Item = Server>>(mut self, servers: I) -> Self {
        set_value!(self servers Some(servers.into_iter().collect()))
    }

    /// Append list of [`Parameter`]s common to all [`Operation`]s to this [`PathItem`].
    pub fn parameters<I: IntoIterator<Item = Parameter>>(mut self, parameters: I) -> Self {
        set_value!(self parameters Some(parameters.into_iter().collect()))
    }
}

/// Path item operation type.
#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Debug)]
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
