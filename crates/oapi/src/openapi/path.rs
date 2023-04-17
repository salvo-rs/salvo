//! Implements [OpenAPI Path Object][paths] types.
//!
//! [paths]: https://spec.openapis.org/oas/latest.html#paths-object
use std::{collections::BTreeMap, iter};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    request_body::RequestBody,
    response::{Response, Responses},
    set_value, Deprecated, ExternalDocs, RefOr, Required, Schema, SecurityRequirement, Server,
};

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
    /// # use salvo_oapi::openapi::path::{Paths, PathItemType};
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
    /// # use salvo_oapi::openapi::path::{Paths, PathItemType};
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

/// Implements [OpenAPI Operation Object][operation] object.
///
/// [operation]: https://spec.openapis.org/oas/latest.html#operation-object
#[non_exhaustive]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<Parameter>>,

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<Vec<SecurityRequirement>>,

    /// Alternative [`Server`]s for this [`Operation`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<Server>>,
}

impl Operation {
    /// Construct a new API [`Operation`].
    pub fn new() -> Self {
        Default::default()
    }
    /// Add or change tags of the [`Operation`].
    pub fn tags<I: IntoIterator<Item = String>>(mut self, tags: I) -> Self {
        set_value!(self tags Some(tags.into_iter().collect()))
    }

    /// Append tag to [`Operation`] tags.
    pub fn tag<S: Into<String>>(mut self, tag: S) -> Self {
        let tag_string = tag.into();
        match self.tags {
            Some(ref mut tags) => tags.push(tag_string),
            None => {
                self.tags = Some(vec![tag_string]);
            }
        }

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
        self.parameters = Some({
            if let Some(mut params) = self.parameters {
                params.extend(parameters.into_iter().map(|parameter| parameter.into()));
                params
            } else {
                parameters.into_iter().map(|parameter| parameter.into()).collect()
            }
        });

        self
    }

    /// Append parameter to [`Operation`] parameters.
    pub fn parameter<P: Into<Parameter>>(mut self, parameter: P) -> Self {
        match self.parameters {
            Some(ref mut parameters) => parameters.push(parameter.into()),
            None => {
                self.parameters = Some(vec![parameter.into()]);
            }
        }

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
    pub fn response<S: Into<String>, R: Into<RefOr<Response>>>(mut self, code: S, response: R) -> Self {
        self.responses.responses.insert(code.into(), response.into());

        self
    }

    /// Add or change deprecated status of the [`Operation`].
    pub fn deprecated(mut self, deprecated: Option<Deprecated>) -> Self {
        set_value!(self deprecated deprecated)
    }

    /// Add or change list of [`SecurityRequirement`]s that are available for [`Operation`].
    pub fn securities<I: IntoIterator<Item = SecurityRequirement>>(mut self, securities: I) -> Self {
        set_value!(self security Some(securities.into_iter().collect()))
    }

    /// Append [`SecurityRequirement`] to [`Operation`] security requirements.
    pub fn security(mut self, security: SecurityRequirement) -> Self {
        if let Some(ref mut securities) = self.security {
            securities.push(security);
        } else {
            self.security = Some(vec![security]);
        }

        self
    }

    /// Add or change list of [`Server`]s of the [`Operation`].
    pub fn servers<I: IntoIterator<Item = Server>>(mut self, servers: I) -> Self {
        set_value!(self servers Some(servers.into_iter().collect()))
    }

    /// Append a new [`Server`] to the [`Operation`] servers.
    pub fn server(mut self, server: Server) -> Self {
        if let Some(ref mut servers) = self.servers {
            servers.push(server);
        } else {
            self.servers = Some(vec![server]);
        }

        self
    }
}

/// Implements [OpenAPI Parameter Object][parameter] for [`Operation`].
///
/// [parameter]: https://spec.openapis.org/oas/latest.html#parameter-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Parameter {
    /// Name of the parameter.
    ///
    /// * For [`ParameterIn::Path`] this must in accordance to path templating.
    /// * For [`ParameterIn::Query`] `Content-Type` or `Authorization` value will be ignored.
    pub name: String,

    /// Parameter location.
    #[serde(rename = "in")]
    pub parameter_in: ParameterIn,

    /// Markdown supported description of the parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Declares whether the parameter is required or not for api.
    ///
    /// * For [`ParameterIn::Path`] this must and will be [`Required::True`].
    pub required: Required,

    /// Declares the parameter deprecated status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Deprecated>,
    // pub allow_empty_value: bool, this is going to be removed from further open api spec releases
    /// Schema of the parameter. Typically [`Schema::Object`] is used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<RefOr<Schema>>,

    /// Describes how [`Parameter`] is being serialized depending on [`Parameter::schema`] (type of a content).
    /// Default value is based on [`ParameterIn`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<ParameterStyle>,

    /// When _`true`_ it will generate separate parameter value for each parameter with _`array`_ and _`object`_ type.
    /// This is also _`true`_ by default for [`ParameterStyle::Form`].
    ///
    /// With explode _`false`_:
    /// ```text
    ///color=blue,black,brown
    /// ```
    ///
    /// With explode _`true`_:
    /// ```text
    ///color=blue&color=black&color=brown
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explode: Option<bool>,

    /// Defines whether parameter should allow reserved characters defined by
    /// [RFC3986](https://tools.ietf.org/html/rfc3986#section-2.2) _`:/?#[]@!$&'()*+,;=`_.
    /// This is only applicable with [`ParameterIn::Query`]. Default value is _`false`_.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_reserved: Option<bool>,

    /// Example of [`Parameter`]'s potential value. This examples will override example
    /// within [`Parameter::schema`] if defined.
    #[serde(skip_serializing_if = "Option::is_none")]
    example: Option<Value>,
}

impl Parameter {
    /// Constructs a new required [`Parameter`] with given name.
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            required: Required::True,
            ..Default::default()
        }
    }
    /// Add name of the [`Parameter`].
    pub fn name<I: Into<String>>(mut self, name: I) -> Self {
        set_value!(self name name.into())
    }

    /// Add in of the [`Parameter`].
    pub fn parameter_in(mut self, parameter_in: ParameterIn) -> Self {
        set_value!(self parameter_in parameter_in)
    }

    /// Add required declaration of the [`Parameter`]. If [`ParameterIn::Path`] is
    /// defined this is always [`Required::True`].
    pub fn required(mut self, required: Required) -> Self {
        self.required = required;
        // required must be true, if parameter_in is Path
        if self.parameter_in == ParameterIn::Path {
            self.required = Required::True;
        }

        self
    }

    /// Add or change description of the [`Parameter`].
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        set_value!(self description Some(description.into()))
    }

    /// Add or change [`Parameter`] deprecated declaration.
    pub fn deprecated(mut self, deprecated: Option<Deprecated>) -> Self {
        set_value!(self deprecated deprecated)
    }

    /// Add or change [`Parameter`]s schema.
    pub fn schema<I: Into<RefOr<Schema>>>(mut self, component: I) -> Self {
        set_value!(self schema Some(component.into()))
    }

    /// Add or change serialization style of [`Parameter`].
    pub fn style(mut self, style: Option<ParameterStyle>) -> Self {
        set_value!(self style style)
    }

    /// Define whether [`Parameter`]s are exploded or not.
    pub fn explode(mut self, explode: Option<bool>) -> Self {
        set_value!(self explode explode)
    }

    /// Add or change whether [`Parameter`] should allow reserved characters.
    pub fn allow_reserved(mut self, allow_reserved: Option<bool>) -> Self {
        set_value!(self allow_reserved allow_reserved)
    }

    /// Add or change example of [`Parameter`]'s potential value.
    pub fn example(mut self, example: Option<Value>) -> Self {
        set_value!(self example example)
    }
}

/// In definition of [`Parameter`].
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ParameterIn {
    /// Declares that parameter is used as query parameter.
    Query,
    /// Declares that parameter is used as path parameter.
    Path,
    /// Declares that parameter is used as header value.
    Header,
    /// Declares that parameter is used as cookie value.
    Cookie,
}

impl Default for ParameterIn {
    fn default() -> Self {
        Self::Path
    }
}

/// Defines how [`Parameter`] should be serialized.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum ParameterStyle {
    /// Path style parameters defined by [RFC6570](https://tools.ietf.org/html/rfc6570#section-3.2.7)
    /// e.g _`;color=blue`_.
    /// Allowed with [`ParameterIn::Path`].
    Matrix,
    /// Label style parameters defined by [RFC6570](https://datatracker.ietf.org/doc/html/rfc6570#section-3.2.5)
    /// e.g _`.color=blue`_.
    /// Allowed with [`ParameterIn::Path`].
    Label,
    /// Form style parameters defined by [RFC6570](https://datatracker.ietf.org/doc/html/rfc6570#section-3.2.8)
    /// e.g. _`color=blue`_. Default value for [`ParameterIn::Query`] [`ParameterIn::Cookie`].
    /// Allowed with [`ParameterIn::Query`] or [`ParameterIn::Cookie`].
    Form,
    /// Default value for [`ParameterIn::Path`] [`ParameterIn::Header`]. e.g. _`blue`_.
    /// Allowed with [`ParameterIn::Path`] or [`ParameterIn::Header`].
    Simple,
    /// Space separated array values e.g. _`blue%20black%20brown`_.
    /// Allowed with [`ParameterIn::Query`].
    SpaceDelimited,
    /// Pipe separated array values e.g. _`blue|black|brown`_.
    /// Allowed with [`ParameterIn::Query`].
    PipeDelimited,
    /// Simple way of rendering nested objects using form parameters .e.g. _`color[B]=150`_.
    /// Allowed with [`ParameterIn::Query`].
    DeepObject,
}

#[cfg(test)]
mod tests {
    use super::{Operation, Operation};
    use crate::openapi::{security::SecurityRequirement, server::Server};

    #[test]
    fn operation_new() {
        let operation = Operation::new();

        assert!(operation.tags.is_none());
        assert!(operation.summary.is_none());
        assert!(operation.description.is_none());
        assert!(operation.operation_id.is_none());
        assert!(operation.external_docs.is_none());
        assert!(operation.parameters.is_none());
        assert!(operation.request_body.is_none());
        assert!(operation.responses.responses.is_empty());
        assert!(operation.callbacks.is_none());
        assert!(operation.deprecated.is_none());
        assert!(operation.security.is_none());
        assert!(operation.servers.is_none());
    }

    #[test]
    fn operation_builder_security() {
        let security_requirement1 = SecurityRequirement::new("api_oauth2_flow", ["edit:items", "read:items"]);
        let security_requirement2 = SecurityRequirement::new("api_oauth2_flow", ["remove:items"]);
        let operation = Operation::new()
            .security(security_requirement1)
            .security(security_requirement2)
            .build();

        assert!(operation.security.is_some());
    }

    #[test]
    fn operation_builder_server() {
        let server1 = Server::new("/api");
        let server2 = Server::new("/admin");
        let operation = Operation::new().server(server1).server(server2).build();

        assert!(operation.servers.is_some());
    }
}
