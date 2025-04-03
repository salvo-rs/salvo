//! Implements [OpenAPI Parameter Object][parameter] types.
//!
//! [parameter]: https://spec.openapis.org/oas/latest.html#parameter-object
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Deprecated, RefOr, Required, Schema};
use crate::PropMap;

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
    /// Returns `true` if instance contains a parameter with the given name and location.
    pub fn contains(&self, name: &str, parameter_in: ParameterIn) -> bool {
        self.0
            .iter()
            .any(|item| item.name == name && item.parameter_in == parameter_in)
    }
    /// Inserts a parameter into the instance.
    pub fn insert<P: Into<Parameter>>(&mut self, parameter: P) {
        let parameter = parameter.into();
        let exist_item = self.0.iter_mut().find(|item| {
            item.name == parameter.name && item.parameter_in == parameter.parameter_in
        });

        if let Some(exist_item) = exist_item {
            exist_item.merge(parameter);
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

    /// Optional extensions "x-something"
    #[serde(skip_serializing_if = "PropMap::is_empty", flatten)]
    pub extensions: PropMap<String, serde_json::Value>,
}

impl Parameter {
    /// Constructs a new required [`Parameter`] with given name.
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            required: Required::Unset,
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
        if self.parameter_in == ParameterIn::Path {
            self.required = Required::True;
        }
        self
    }

    /// Fill [`Parameter`] with values from another [`Parameter`]. Fields will replaced if it is not set.
    pub fn merge(&mut self, other: Parameter) -> bool {
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
            extensions,
        } = other;
        if name != self.name || parameter_in != self.parameter_in {
            return false;
        }
        if let Some(description) = description {
            self.description = Some(description);
        }

        if required != Required::Unset {
            self.required = required;
        }

        if let Some(deprecated) = deprecated {
            self.deprecated = Some(deprecated);
        }
        if let Some(schema) = schema {
            self.schema = Some(schema);
        }
        if let Some(style) = style {
            self.style = Some(style);
        }
        if let Some(explode) = explode {
            self.explode = Some(explode);
        }
        if let Some(allow_reserved) = allow_reserved {
            self.allow_reserved = Some(allow_reserved);
        }
        if let Some(example) = example {
            self.example = Some(example);
        }

        self.extensions.extend(extensions);
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
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Debug)]
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
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use crate::Object;

    use super::*;

    #[test]
    fn test_build_parameter() {
        let parameter = Parameter::new("name");
        assert_eq!(parameter.name, "name");

        let parameter = parameter
            .name("new name")
            .parameter_in(ParameterIn::Query)
            .required(Required::True)
            .description("description")
            .deprecated(Deprecated::False)
            .schema(Schema::object(Object::new()))
            .style(ParameterStyle::Simple)
            .explode(true)
            .allow_reserved(true)
            .example(Value::String("example".to_string()));
        assert_json_eq!(
            parameter,
            json!({
                "name": "new name",
                "in": "query",
                "required": true,
                "description": "description",
                "deprecated": false,
                "schema": {
                    "type": "object"
                },
                "style": "simple",
                "explode": true,
                "allowReserved": true,
                "example": "example"
            })
        );
    }

    #[test]
    fn test_parameter_merge_fail() {
        let mut parameter1 = Parameter::new("param1");
        let parameter2 = Parameter::new("param2");

        assert!(!parameter1.merge(parameter2));
    }

    #[test]
    fn test_parameter_merge_success() {
        let mut parameter1 = Parameter::new("param1");
        let mut parameter2 = Parameter::new("param1")
            .description("description")
            .required(Required::True)
            .deprecated(Deprecated::True)
            .schema(Schema::object(Object::new()))
            .style(ParameterStyle::Form)
            .explode(true)
            .allow_reserved(true)
            .example(Value::String("example".to_string()));

        parameter1.extensions =
            PropMap::from([("key1".to_string(), Value::String("value1".to_string()))]);
        parameter2.extensions =
            PropMap::from([("key2".to_string(), Value::String("value2".to_string()))]);

        assert!(parameter1.merge(parameter2));
        assert_json_eq!(
            parameter1,
            json!({
                "name": "param1",
                "in": "path",
                "description": "description",
                "required": true,
                "deprecated": true,
                "schema": {
                    "type": "object"
                },
                "style": "form",
                "explode": true,
                "allowReserved": true,
                "example": "example",
                "key1": "value1",
                "key2": "value2"
            })
        )
    }

    #[test]
    fn test_parameter_merge_no_extensions() {
        let mut parameter1 = Parameter::new("param1");
        let mut parameter2 = Parameter::new("param1")
            .description("description")
            .required(Required::True)
            .deprecated(Deprecated::True)
            .schema(Schema::object(Object::new()))
            .style(ParameterStyle::Form)
            .explode(true)
            .allow_reserved(true)
            .example(Value::String("example".to_string()));

        parameter2.extensions =
            PropMap::from([("key2".to_string(), Value::String("value2".to_string()))]);

        assert!(parameter1.merge(parameter2));
        assert_json_eq!(
            parameter1,
            json!({
                "name": "param1",
                "in": "path",
                "description": "description",
                "required": true,
                "deprecated": true,
                "schema": {
                    "type": "object"
                },
                "style": "form",
                "explode": true,
                "allowReserved": true,
                "example": "example",
                "key2": "value2",
            })
        )
    }

    #[test]
    fn test_build_parameters() {
        let parameters = Parameters::new();
        assert!(parameters.is_empty());
    }

    #[test]
    fn test_parameters_into_iter() {
        let parameters = Parameters::new().parameter(Parameter::new("param"));
        let mut iter = parameters.into_iter();
        assert_eq!(iter.next(), Some(Parameter::new("param")));
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_parameters_contain() {
        let parameters = Parameters::new().parameter(Parameter::new("param"));
        assert!(parameters.contains("param", ParameterIn::Path));
    }

    #[test]
    fn test_parameters_insert_existed_item() {
        let mut parameters = Parameters::new();
        parameters.insert(Parameter::new("param"));
        assert!(parameters.contains("param", ParameterIn::Path));

        parameters.insert(Parameter::new("param"));
        assert_eq!(parameters.0.len(), 1);
    }

    #[test]
    fn test_parameters_append() {
        let mut parameters1 = Parameters::new().parameter(Parameter::new("param1"));
        let mut parameters2 = Parameters::new().parameter(Parameter::new("param2"));

        parameters1.append(&mut parameters2);
        assert_json_eq!(
            parameters1,
            json!([
                {
                    "in": "path",
                    "name": "param1",
                    "required": false
                },
                {
                    "in": "path",
                    "name": "param2",
                    "required": false
                }
            ])
        );
    }
}
