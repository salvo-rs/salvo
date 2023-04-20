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