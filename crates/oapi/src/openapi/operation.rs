//! Implements [OpenAPI Operation Object][operation] types.
//!
//! [operation]: https://spec.openapis.org/oas/latest.html#operation-object
use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};

use super::{
    Deprecated, ExternalDocs, RefOr, SecurityRequirement, Server,
    request_body::RequestBody,
    response::{Response, Responses},
};
use crate::{Parameter, Parameters, PathItemType, PropMap, Servers};

/// Collection for save [`Operation`]s.
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
pub struct Operations(pub PropMap<PathItemType, Operation>);
impl Deref for Operations {
    type Target = PropMap<PathItemType, Operation>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for Operations {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl IntoIterator for Operations {
    type Item = (PathItemType, Operation);
    type IntoIter = <PropMap<PathItemType, Operation> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
impl Operations {
    /// Construct a new empty [`Operations`]. This is effectively same as calling [`Operations::default`].
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns `true` if instance contains no elements.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Add a new operation and returns `self`.
    pub fn operation<K: Into<PathItemType>, O: Into<Operation>>(
        mut self,
        item_type: K,
        operation: O,
    ) -> Self {
        self.insert(item_type, operation);
        self
    }

    /// Inserts a key-value pair into the instance.
    pub fn insert<K: Into<PathItemType>, O: Into<Operation>>(
        &mut self,
        item_type: K,
        operation: O,
    ) {
        self.0.insert(item_type.into(), operation.into());
    }

    /// Moves all elements from `other` into `self`, leaving `other` empty.
    ///
    /// If a key from `other` is already present in `self`, the respective
    /// value from `self` will be overwritten with the respective value from `other`.
    pub fn append(&mut self, other: &mut Operations) {
        self.0.append(&mut other.0);
    }
    /// Extends a collection with the contents of an iterator.
    pub fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = (PathItemType, Operation)>,
    {
        for (item_type, operation) in iter {
            self.insert(item_type, operation);
        }
    }
}

/// Implements [OpenAPI Operation Object][operation] object.
///
/// [operation]: https://spec.openapis.org/oas/latest.html#operation-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    /// List of tags used for grouping operations.
    ///
    /// When used with derive [`#[salvo_oapi::endpoint(...)]`][derive_path] attribute macro the default
    /// value used will be resolved from handler path provided in `#[openapi(paths(...))]` with
    /// [`#[derive(OpenApi)]`][derive_openapi] macro. If path resolves to `None` value `crate` will
    /// be used by default.
    ///
    /// [derive_path]: ../../attr.path.html
    /// [derive_openapi]: ../../derive.OpenApi.html
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Short summary what [`Operation`] does.
    ///
    /// When used with derive [`#[salvo_oapi::endpoint(...)]`][derive_path] attribute macro the value
    /// is taken from **first line** of doc comment.
    ///
    /// [derive_path]: ../../attr.path.html
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// Long explanation of [`Operation`] behaviour. Markdown syntax is supported.
    ///
    /// When used with derive [`#[salvo_oapi::endpoint(...)]`][derive_path] attribute macro the
    /// doc comment is used as value for description.
    ///
    /// [derive_path]: ../../attr.path.html
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Unique identifier for the API [`Operation`]. Most typically this is mapped to handler function name.
    ///
    /// When used with derive [`#[salvo_oapi::endpoint(...)]`][derive_path] attribute macro the handler function
    /// name will be used by default.
    ///
    /// [derive_path]: ../../attr.path.html
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,

    /// Additional external documentation for this operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDocs>,

    /// List of applicable parameters for this [`Operation`].
    #[serde(skip_serializing_if = "Parameters::is_empty")]
    pub parameters: Parameters,

    /// Optional request body for this [`Operation`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<RequestBody>,

    /// List of possible responses returned by the [`Operation`].
    pub responses: Responses,

    /// Callback information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callbacks: Option<String>,

    /// Define whether the operation is deprecated or not and thus should be avoided consuming.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Deprecated>,

    /// Declaration which security mechanisms can be used for for the operation. Only one
    /// [`SecurityRequirement`] must be met.
    ///
    /// Security for the [`Operation`] can be set to optional by adding empty security with
    /// [`SecurityRequirement::default`].
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(rename = "security")]
    pub securities: Vec<SecurityRequirement>,

    /// Alternative [`Server`]s for this [`Operation`].
    #[serde(skip_serializing_if = "Servers::is_empty")]
    pub servers: Servers,

    /// Optional extensions "x-something"
    #[serde(skip_serializing_if = "PropMap::is_empty", flatten)]
    pub extensions: PropMap<String, serde_json::Value>,
}

impl Operation {
    /// Construct a new API [`Operation`].
    pub fn new() -> Self {
        Default::default()
    }

    /// Add or change tags of the [`Operation`].
    pub fn tags<I, T>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        self.tags = tags.into_iter().map(|t| t.into()).collect();
        self
    }
    /// Append tag to [`Operation`] tags and returns `Self`.
    pub fn add_tag<S: Into<String>>(mut self, tag: S) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Add or change short summary of the [`Operation`].
    pub fn summary<S: Into<String>>(mut self, summary: S) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Add or change description of the [`Operation`].
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add or change operation id of the [`Operation`].
    pub fn operation_id<S: Into<String>>(mut self, operation_id: S) -> Self {
        self.operation_id = Some(operation_id.into());
        self
    }

    /// Add or change parameters of the [`Operation`].
    pub fn parameters<I: IntoIterator<Item = P>, P: Into<Parameter>>(
        mut self,
        parameters: I,
    ) -> Self {
        self.parameters
            .extend(parameters.into_iter().map(|parameter| parameter.into()));
        self
    }
    /// Append parameter to [`Operation`] parameters and returns `Self`.
    pub fn add_parameter<P: Into<Parameter>>(mut self, parameter: P) -> Self {
        self.parameters.insert(parameter);
        self
    }

    /// Add or change request body of the [`Operation`].
    pub fn request_body(mut self, request_body: RequestBody) -> Self {
        self.request_body = Some(request_body);
        self
    }

    /// Add or change responses of the [`Operation`].
    pub fn responses<R: Into<Responses>>(mut self, responses: R) -> Self {
        self.responses = responses.into();
        self
    }
    /// Append status code and a [`Response`] to the [`Operation`] responses map and returns `Self`.
    ///
    /// * `code` must be valid HTTP status code.
    /// * `response` is instances of [`Response`].
    pub fn add_response<S: Into<String>, R: Into<RefOr<Response>>>(
        mut self,
        code: S,
        response: R,
    ) -> Self {
        self.responses.insert(code, response);
        self
    }

    /// Add or change deprecated status of the [`Operation`].
    pub fn deprecated<D: Into<Deprecated>>(mut self, deprecated: D) -> Self {
        self.deprecated = Some(deprecated.into());
        self
    }

    /// Add or change list of [`SecurityRequirement`]s that are available for [`Operation`].
    pub fn securities<I: IntoIterator<Item = SecurityRequirement>>(
        mut self,
        securities: I,
    ) -> Self {
        self.securities = securities.into_iter().collect();
        self
    }
    /// Append [`SecurityRequirement`] to [`Operation`] security requirements and returns `Self`.
    pub fn add_security(mut self, security: SecurityRequirement) -> Self {
        self.securities.push(security);
        self
    }

    /// Add or change list of [`Server`]s of the [`Operation`].
    pub fn servers<I: IntoIterator<Item = Server>>(mut self, servers: I) -> Self {
        self.servers = Servers(servers.into_iter().collect());
        self
    }
    /// Append a new [`Server`] to the [`Operation`] servers and returns `Self`.
    pub fn add_server(mut self, server: Server) -> Self {
        self.servers.insert(server);
        self
    }

    /// For easy chaining of operations.
    pub fn then<F>(self, func: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        func(self)
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::{Operation, Operations};
    use crate::{
        Deprecated, Parameter, PathItemType, RequestBody, Responses, security::SecurityRequirement,
        server::Server,
    };

    #[test]
    fn operation_new() {
        let operation = Operation::new();

        assert!(operation.tags.is_empty());
        assert!(operation.summary.is_none());
        assert!(operation.description.is_none());
        assert!(operation.operation_id.is_none());
        assert!(operation.external_docs.is_none());
        assert!(operation.parameters.is_empty());
        assert!(operation.request_body.is_none());
        assert!(operation.responses.is_empty());
        assert!(operation.callbacks.is_none());
        assert!(operation.deprecated.is_none());
        assert!(operation.securities.is_empty());
        assert!(operation.servers.is_empty());
    }

    #[test]
    fn test_build_operation() {
        let operation = Operation::new()
            .tags(["tag1", "tag2"])
            .add_tag("tag3")
            .summary("summary")
            .description("description")
            .operation_id("operation_id")
            .parameters([Parameter::new("param1")])
            .add_parameter(Parameter::new("param2"))
            .request_body(RequestBody::new())
            .responses(Responses::new())
            .deprecated(Deprecated::False)
            .securities([SecurityRequirement::new("api_key", ["read:items"])])
            .servers([Server::new("/api")]);

        assert_json_eq!(
            operation,
            json!({
                "responses": {},
                "parameters": [
                    {
                        "name": "param1",
                        "in": "path",
                        "required": false
                    },
                    {
                        "name": "param2",
                        "in": "path",
                        "required": false
                    }
                ],
                "operationId": "operation_id",
                "deprecated": false,
                "security": [
                    {
                        "api_key": ["read:items"]
                    }
                ],
                "servers": [{"url": "/api"}],
                "summary": "summary",
                "tags": ["tag1", "tag2", "tag3"],
                "description": "description",
                "requestBody": {
                    "content": {}
                }
            })
        );
    }

    #[test]
    fn operation_security() {
        let security_requirement1 =
            SecurityRequirement::new("api_oauth2_flow", ["edit:items", "read:items"]);
        let security_requirement2 = SecurityRequirement::new("api_oauth2_flow", ["remove:items"]);
        let operation = Operation::new()
            .add_security(security_requirement1)
            .add_security(security_requirement2);

        assert!(!operation.securities.is_empty());
    }

    #[test]
    fn operation_server() {
        let server1 = Server::new("/api");
        let server2 = Server::new("/admin");
        let operation = Operation::new().add_server(server1).add_server(server2);
        assert!(!operation.servers.is_empty());
    }

    #[test]
    fn test_operations() {
        let operations = Operations::new();
        assert!(operations.is_empty());

        let mut operations = operations.operation(PathItemType::Get, Operation::new());
        operations.insert(PathItemType::Post, Operation::new());
        operations.extend([(PathItemType::Head, Operation::new())]);
        assert_eq!(3, operations.len());
    }

    #[test]
    fn test_operations_into_iter() {
        let mut operations = Operations::new();
        operations.insert(PathItemType::Get, Operation::new());
        operations.insert(PathItemType::Post, Operation::new());
        operations.insert(PathItemType::Head, Operation::new());

        let mut iter = operations.into_iter();
        assert_eq!((PathItemType::Get, Operation::new()), iter.next().unwrap());
        assert_eq!((PathItemType::Post, Operation::new()), iter.next().unwrap());
        assert_eq!((PathItemType::Head, Operation::new()), iter.next().unwrap());
    }

    #[test]
    fn test_operations_then() {
        let print_operation = |operation: Operation| {
            println!("{:?}", operation);
            operation
        };
        let operation = Operation::new();

        operation.then(print_operation);
    }
}
