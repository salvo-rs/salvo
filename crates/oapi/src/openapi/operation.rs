//! Implements [OpenAPI Operation Object][operation] types.
//!
//! [operation]: https://spec.openapis.org/oas/latest.html#operation-object
use serde::{Deserialize, Serialize};

use super::{
    request_body::RequestBody,
    response::{Response, Responses},
    set_value, Deprecated, ExternalDocs, RefOr, SecurityRequirement, Server,
};
use crate::{Parameter, Parameters};

/// Implements [OpenAPI Operation Object][operation] object.
///
/// [operation]: https://spec.openapis.org/oas/latest.html#operation-object
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    /// List of tags used for grouping operations.
    ///
    /// When used with derive [`#[salvo_oapi::path(...)]`][derive_path] attribute macro the default
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
    /// When used with derive [`#[salvo_oapi::path(...)]`][derive_path] attribute macro the value
    /// is taken from **first line** of doc comment.
    ///
    /// [derive_path]: ../../attr.path.html
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// Long explanation of [`Operation`] behaviour. Markdown syntax is supported.
    ///
    /// When used with derive [`#[salvo_oapi::path(...)]`][derive_path] attribute macro the
    /// doc comment is used as value for description.
    ///
    /// [derive_path]: ../../attr.path.html
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Unique identifier for the API [`Operation`]. Most typically this is mapped to handler function name.
    ///
    /// When used with derive [`#[salvo_oapi::path(...)]`][derive_path] attribute macro the handler function
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

    // TODO
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<Server>,
}

impl Operation {
    /// Construct a new API [`Operation`].
    pub fn new() -> Self {
        Default::default()
    }

    /// Add or change tags of the [`Operation`].
    pub fn tags<I: IntoIterator<Item = String>>(mut self, tags: I) -> Self {
        set_value!(self tags tags.into_iter().collect())
    }

    /// Append tag to [`Operation`] tags.
    pub fn add_tag<S: Into<String>>(mut self, tag: S) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Add or change short summary of the [`Operation`].
    pub fn summary<S: Into<String>>(mut self, summary: S) -> Self {
        set_value!(self summary Some(summary.into()))
    }

    /// Add or change description of the [`Operation`].
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        set_value!(self description Some( description.into()))
    }

    /// Add or change operation id of the [`Operation`].
    pub fn operation_id<S: Into<String>>(mut self, operation_id: S) -> Self {
        set_value!(self operation_id Some(operation_id.into()))
    }

    /// Add or change parameters of the [`Operation`].
    pub fn parameters<I: IntoIterator<Item = P>, P: Into<Parameter>>(mut self, parameters: I) -> Self {
        self.parameters
            .extend(parameters.into_iter().map(|parameter| parameter.into()));
        self
    }

    /// Append parameter to [`Operation`] parameters.
    pub fn add_parameter<P: Into<Parameter>>(mut self, parameter: P) -> Self {
        self.parameters.append(parameter.into());
        self
    }

    /// Add or change request body of the [`Operation`].
    pub fn request_body(mut self, request_body: RequestBody) -> Self {
        set_value!(self request_body Some(request_body))
    }

    /// Add or change responses of the [`Operation`].
    pub fn responses<R: Into<Responses>>(mut self, responses: R) -> Self {
        set_value!(self responses responses.into())
    }

    /// Append status code and a [`Response`] to the [`Operation`] responses map.
    ///
    /// * `code` must be valid HTTP status code.
    /// * `response` is instances of [`Response`].
    pub fn add_response<S: Into<String>, R: Into<RefOr<Response>>>(mut self, code: S, response: R) -> Self {
        self.responses.insert(code.into(), response.into());
        self
    }

    /// Add or change deprecated status of the [`Operation`].
    pub fn deprecated(mut self, deprecated: Option<Deprecated>) -> Self {
        set_value!(self deprecated deprecated)
    }

    /// Add or change list of [`SecurityRequirement`]s that are available for [`Operation`].
    pub fn securities<I: IntoIterator<Item = SecurityRequirement>>(mut self, securities: I) -> Self {
        set_value!(self securities securities.into_iter().collect())
    }

    /// Append [`SecurityRequirement`] to [`Operation`] security requirements.
    pub fn add_security(mut self, security: SecurityRequirement) -> Self {
        self.securities.push(security);
        self
    }

    /// Add or change list of [`Server`]s of the [`Operation`].
    pub fn servers<I: IntoIterator<Item = Server>>(mut self, servers: I) -> Self {
        set_value!(self servers servers.into_iter().collect())
    }

    /// Append a new [`Server`] to the [`Operation`] servers.
    pub fn add_server(mut self, server: Server) -> Self {
        self.servers.push(server);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::Operation;
    use crate::{security::SecurityRequirement, server::Server};

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
    fn operation_builder_security() {
        let security_requirement1 = SecurityRequirement::new("api_oauth2_flow", ["edit:items", "read:items"]);
        let security_requirement2 = SecurityRequirement::new("api_oauth2_flow", ["remove:items"]);
        let operation = Operation::new()
            .add_security(security_requirement1)
            .add_security(security_requirement2);

        assert!(operation.securities.is_empty());
    }

    #[test]
    fn operation_builder_server() {
        let server1 = Server::new("/api");
        let server2 = Server::new("/admin");
        let operation = Operation::new().add_server(server1).add_server(server2);

        assert!(operation.servers.is_empty());
    }
}
