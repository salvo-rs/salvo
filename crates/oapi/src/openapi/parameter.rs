//! Implements [OpenAPI Parameter Object][parameter] types.
//!
//! [parameter]: https://spec.openapis.org/oas/latest.html#parameter-object
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Deprecated, RefOr, Required, Schema};

/// Collection for OpenAPI Parameter Objects.
#[derive(Serialize, Deserialize, Debug, PartialEq, Default, Clone)]
pub struct Parameters(pub Vec<Parameter>);

impl IntoIterator for Parameters {
    type Item = Parameter;
    type IntoIter = <Vec<Parameter> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Parameters {
    /// Construct a new empty [`Parameters`]. This is effectively same as calling [`Parameters::default`].
    pub fn new() -> Self {
        Default::default()
    }
    /// Returns `true` if instance contains no elements.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Add a new paramater and returns `self`.
    pub fn parameter<P: Into<Parameter>>(mut self, parameter: P) -> Self {
        self.insert(parameter);
        self
    }
    /// Inserts a parameter into the instance.
    pub fn insert<P: Into<Parameter>>(&mut self, parameter: P) {
        let parameter = parameter.into();
        let exist_item = self
            .0
            .iter_mut()
            .find(|item| item.name == parameter.name && item.parameter_in == parameter.parameter_in);

        if let Some(exist_item) = exist_item {
            exist_item.fill_with(parameter);
        } else {
            self.0.push(parameter);
        }
    }
    /// Moves all elements from `other` into `self`, leaving `other` empty.
    ///
    /// If a key from `other` is already present in `self`, the respective
    /// value from `self` will be overwritten with the respective value from `other`.
    pub fn append(&mut self, other: &mut Parameters) {
        for item in other.0.drain(..) {
            self.insert(item);
        }
    }
    /// Extends a collection with the contents of an iterator.
    pub fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = Parameter>,
    {
        for item in iter {
            self.insert(item);
        }
    }
}

/// Implements [OpenAPI Parameter Object][parameter] for [`Operation`](struct.Operation).
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
        self.name = name.into();
        self
    }

    /// Add in of the [`Parameter`].
    pub fn parameter_in(mut self, parameter_in: ParameterIn) -> Self {
        self.parameter_in = parameter_in;
        self
    }

    /// Fill [`Parameter`] with values from another [`Parameter`]. Fields will replaced if it is not set.
    pub fn fill_with(&mut self, other: Parameter) -> bool {
        let Parameter {
            name,
            parameter_in,
            description,
            required,
            deprecated,
            schema,
            style,
            explode,
            allow_reserved,
            example,
        } = other;
        if name != self.name || parameter_in != self.parameter_in {
            return false;
        }
        if self.description.is_none() {
            self.description = description;
        }

        self.required = required;

        if self.deprecated.is_none() {
            self.deprecated = deprecated;
        }
        if self.schema.is_none() {
            self.schema = schema;
        }
        if self.style.is_none() {
            self.style = style;
        }
        if self.explode.is_none() {
            self.explode = explode;
        }
        if self.allow_reserved.is_none() {
            self.allow_reserved = allow_reserved;
        }
        if self.example.is_none() {
            self.example = example;
        }
        true
    }

    /// Add required declaration of the [`Parameter`]. If [`ParameterIn::Path`] is
    /// defined this is always [`Required::True`].
    pub fn required(mut self, required: impl Into<Required>) -> Self {
        self.required = required.into();
        // required must be true, if parameter_in is Path
        if self.parameter_in == ParameterIn::Path {
            self.required = Required::True;
        }

        self
    }

    /// Add or change description of the [`Parameter`].
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add or change [`Parameter`] deprecated declaration.
    pub fn deprecated<D: Into<Deprecated>>(mut self, deprecated: D) -> Self {
        self.deprecated = Some(deprecated.into());
        self
    }

    /// Add or change [`Parameter`]s schema.
    pub fn schema<I: Into<RefOr<Schema>>>(mut self, component: I) -> Self {
        self.schema = Some(component.into());
        self
    }

    /// Add or change serialization style of [`Parameter`].
    pub fn style(mut self, style: ParameterStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Define whether [`Parameter`]s are exploded or not.
    pub fn explode(mut self, explode: bool) -> Self {
        self.explode = Some(explode);
        self
    }

    /// Add or change whether [`Parameter`] should allow reserved characters.
    pub fn allow_reserved(mut self, allow_reserved: bool) -> Self {
        self.allow_reserved = Some(allow_reserved);
        self
    }

    /// Add or change example of [`Parameter`]'s potential value.
    pub fn example(mut self, example: Value) -> Self {
        self.example = Some(example);
        self
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
