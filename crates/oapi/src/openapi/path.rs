//! Implements [OpenAPI Path Object][paths] types.
//!
//! [paths]: https://spec.openapis.org/oas/latest.html#paths-object
use std::iter;
use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};

use super::{Operation, Operations, Parameter, Parameters, PathMap, PropMap, Server, Servers};

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
    #[must_use]
    pub fn new() -> Self {
        Default::default()
    }
    /// Inserts a key-value pair into the instance and returns `self`.
    #[must_use]
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
                if value.ref_location.is_some() {
                    item.ref_location = value.ref_location.take();
                }
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
    pub fn append(&mut self, other: &mut Self) {
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
    /// External reference to a Path Item Object defined elsewhere.
    ///
    /// In OpenAPI 3.1 a Path Item Object can carry its own `$ref` field that delegates to
    /// another Path Item definition. When set, sibling fields' behavior is undefined per
    /// spec — most consumers resolve the reference and ignore them.
    ///
    /// See <https://spec.openapis.org/oas/v3.1.0#path-item-object>.
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none", default)]
    pub ref_location: Option<String>,

    /// Optional summary intended to apply all operations in this [`PathItem`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// Optional description intended to apply all operations in this [`PathItem`].
    /// Description supports markdown syntax.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Alternative [`Server`] array to serve all [`Operation`]s in this [`PathItem`] overriding
    /// the global server array.
    #[serde(skip_serializing_if = "Servers::is_empty", default)]
    pub servers: Servers,

    /// List of [`Parameter`]s common to all [`Operation`]s in this [`PathItem`]. Parameters cannot
    /// contain duplicate parameters. They can be overridden in [`Operation`] level but cannot be
    /// removed there.
    #[serde(skip_serializing_if = "Parameters::is_empty", default)]
    pub parameters: Parameters,

    /// Map of operations in this [`PathItem`]. Operations can hold only one operation
    /// per [`PathItemType`].
    #[serde(flatten, default)]
    pub operations: Operations,

    /// Optional extensions "x-something"
    #[serde(skip_serializing_if = "PropMap::is_empty", flatten)]
    pub extensions: PropMap<String, serde_json::Value>,
}

impl PathItem {
    /// Construct a new [`PathItem`] with provided [`Operation`] mapped to given [`PathItemType`].
    pub fn new<O: Into<Operation>>(path_item_type: PathItemType, operation: O) -> Self {
        let operations = PropMap::from_iter(iter::once((path_item_type, operation.into())));

        Self {
            operations: Operations(operations),
            ..Default::default()
        }
    }

    /// Construct a [`PathItem`] that is purely a reference to another Path Item, e.g. one
    /// defined under `components.pathItems`.
    ///
    /// ```
    /// # use salvo_oapi::PathItem;
    /// let item = PathItem::from_ref("#/components/pathItems/PingWebhook");
    /// ```
    #[must_use]
    pub fn from_ref<S: Into<String>>(ref_location: S) -> Self {
        Self {
            ref_location: Some(ref_location.into()),
            ..Default::default()
        }
    }

    /// Set the `$ref` location for this [`PathItem`] and return `self`.
    #[must_use]
    pub fn ref_location<S: Into<String>>(mut self, ref_location: S) -> Self {
        self.ref_location = Some(ref_location.into());
        self
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
        if other.ref_location.is_some() {
            self.ref_location = other.ref_location.take();
        }
        other
            .extensions
            .retain(|name, _| !self.extensions.contains_key(name));
        self.extensions.append(&mut other.extensions);
    }

    /// Append a new [`Operation`] by [`PathItemType`] to this [`PathItem`]. Operations can
    /// hold only one operation per [`PathItemType`].
    #[must_use]
    pub fn add_operation<O: Into<Operation>>(
        mut self,
        path_item_type: PathItemType,
        operation: O,
    ) -> Self {
        self.operations.insert(path_item_type, operation.into());
        self
    }

    /// Add or change summary intended to apply all operations in this [`PathItem`].
    #[must_use]
    pub fn summary<S: Into<String>>(mut self, summary: S) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Add or change optional description intended to apply all operations in this [`PathItem`].
    /// Description supports markdown syntax.
    #[must_use]
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add list of alternative [`Server`]s to serve all [`Operation`]s in this [`PathItem`]
    /// overriding the global server array.
    #[must_use]
    pub fn servers<I: IntoIterator<Item = Server>>(mut self, servers: I) -> Self {
        self.servers = Servers(servers.into_iter().collect());
        self
    }

    /// Append list of [`Parameter`]s common to all [`Operation`]s to this [`PathItem`].
    #[must_use]
    pub fn parameters<I: IntoIterator<Item = Parameter>>(mut self, parameters: I) -> Self {
        self.parameters = Parameters(parameters.into_iter().collect());
        self
    }

    /// Add openapi extensions (`x-something`) for [`PathItem`].
    #[must_use]
    pub fn extensions(mut self, extensions: PropMap<String, serde_json::Value>) -> Self {
        self.extensions = extensions;
        self
    }
}

/// Path item operation type.
///
/// Mirrors the HTTP methods supported by the OpenAPI 3.1 [Path Item Object][path_item];
/// note that the spec deliberately does not list `CONNECT`, so it is intentionally absent
/// here as well.
///
/// [path_item]: https://spec.openapis.org/oas/latest.html#path-item-object
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
    fn path_item_ref_serializes_as_dollar_ref() {
        let item = PathItem::from_ref("#/components/pathItems/PingWebhook");
        assert_json_eq!(
            item,
            json!({ "$ref": "#/components/pathItems/PingWebhook" })
        );
    }

    #[test]
    fn path_item_ref_round_trips_via_serde() {
        let raw = json!({ "$ref": "#/components/pathItems/PingWebhook" });
        let item: PathItem = serde_json::from_value(raw.clone()).expect("deserialize");
        assert_eq!(
            item.ref_location.as_deref(),
            Some("#/components/pathItems/PingWebhook")
        );
        // Other fields are absent — sibling fields with $ref are spec-undefined, so we
        // expect an otherwise empty PathItem.
        assert!(item.summary.is_none());
        assert!(item.operations.is_empty());

        let reserialized = serde_json::to_value(&item).expect("serialize");
        assert_eq!(reserialized, raw);
    }

    #[test]
    fn path_item_ref_setter_overrides_plain_construction() {
        let item = PathItem::new(PathItemType::Get, Operation::new())
            .ref_location("#/components/pathItems/Other");

        let value = serde_json::to_value(&item).expect("serialize");
        assert_eq!(
            value["$ref"],
            json!("#/components/pathItems/Other"),
            "$ref should serialize when set"
        );
        // The previously-attached operation is still there in this object form;
        // consumers will resolve $ref and ignore siblings, but the type doesn't
        // suppress them.
        assert!(value.get("get").is_some());
    }

    #[test]
    fn path_item_parameters_serialize_as_named_field() {
        use crate::Parameter;

        let path_item = PathItem::new(PathItemType::Get, Operation::new())
            .parameters([Parameter::new("id").parameter_in(crate::ParameterIn::Path)]);

        assert_json_eq!(
            path_item,
            json!({
                "parameters": [
                    {
                        "name": "id",
                        "in": "path",
                        "required": true
                    }
                ],
                "get": {
                    "responses": {}
                }
            })
        );
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
