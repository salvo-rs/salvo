//! Implements [OpenAPI Parameter Object][parameter] types.
//!
//! [parameter]: https://spec.openapis.org/oas/latest.html#parameter-object
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::content::Content;
use super::example::Example;
use super::{Deprecated, Ref, RefOr, Required, Schema};
use crate::PropMap;

fn is_required_unset(value: &Required) -> bool {
    matches!(value, Required::Unset)
}

/// Collection of inline OpenAPI Parameter Objects and references to reusable parameters.
///
/// Inline entries are identified by name and location. References are identified by their
/// reference location; they are retained without resolving their target, including external
/// references. Consequently an inline entry and a reference remain distinct entries.
#[derive(Serialize, Deserialize, Debug, PartialEq, Default, Clone)]
pub struct Parameters(pub Vec<RefOr<Parameter>>);

impl IntoIterator for Parameters {
    type Item = RefOr<Parameter>;
    type IntoIter = <Vec<RefOr<Parameter>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<P: Into<RefOr<Parameter>>> From<Vec<P>> for Parameters {
    fn from(parameters: Vec<P>) -> Self {
        let mut result = Self::new();
        result.extend(parameters);
        result
    }
}

impl From<Ref> for RefOr<Parameter> {
    fn from(reference: Ref) -> Self {
        Self::Ref(reference)
    }
}

impl Parameters {
    /// Construct a new empty [`Parameters`]. This is effectively same as calling
    /// [`Parameters::default`].
    #[must_use]
    pub fn new() -> Self {
        Default::default()
    }
    /// Returns `true` if instance contains no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Add a new parameter and returns `self`.
    #[must_use]
    pub fn parameter<P: Into<RefOr<Parameter>>>(mut self, parameter: P) -> Self {
        self.insert(parameter);
        self
    }
    /// Iterate over inline parameters, skipping references without resolving their targets.
    pub fn iter_inline(&self) -> impl Iterator<Item = &Parameter> {
        self.0.iter().filter_map(|parameter| match parameter {
            RefOr::Type(parameter) => Some(parameter),
            RefOr::Ref(_) => None,
        })
    }

    /// Mutably iterate over inline parameters, skipping references.
    pub fn iter_inline_mut(&mut self) -> impl Iterator<Item = &mut Parameter> {
        self.0.iter_mut().filter_map(|parameter| match parameter {
            RefOr::Type(parameter) => Some(parameter),
            RefOr::Ref(_) => None,
        })
    }

    /// Returns `true` if an inline parameter has the given name and location.
    /// References are not resolved by this method.
    #[must_use]
    pub fn contains(&self, name: &str, parameter_in: ParameterIn) -> bool {
        self.iter_inline()
            .any(|item| item.name == name && item.parameter_in == parameter_in)
    }
    /// Insert an inline parameter or reference.
    ///
    /// Matching inline entries are merged by name and location. A reference replaces an
    /// existing reference with the same location, including its sibling metadata.
    pub fn insert<P: Into<RefOr<Parameter>>>(&mut self, parameter: P) {
        match parameter.into() {
            RefOr::Type(mut parameter) => {
                // Per the OpenAPI 3.1 spec, path parameters MUST have `required: true`.
                if parameter.parameter_in == ParameterIn::Path {
                    parameter.required = Required::True;
                }
                let existing = self.iter_inline_mut().find(|item| {
                    item.name == parameter.name && item.parameter_in == parameter.parameter_in
                });
                if let Some(existing) = existing {
                    existing.merge(parameter);
                    return;
                }
                self.0.push(RefOr::Type(parameter));
            }
            RefOr::Ref(reference) => {
                let existing = self.0.iter_mut().find(|item| {
                    matches!(item, RefOr::Ref(existing) if existing.ref_location == reference.ref_location)
                });
                if let Some(existing) = existing {
                    *existing = RefOr::Ref(reference);
                } else {
                    self.0.push(RefOr::Ref(reference));
                }
            }
        }
    }
    /// Moves all elements from `other` into `self`, leaving `other` empty.
    ///
    /// If a key from `other` is already present in `self`, the respective
    /// value from `self` will be overwritten with the respective value from `other`.
    pub fn append(&mut self, other: &mut Self) {
        for item in other.0.drain(..) {
            self.insert(item);
        }
    }
    /// Extends a collection with the contents of an iterator.
    pub fn extend<I, P>(&mut self, iter: I)
    where
        I: IntoIterator<Item = P>,
        P: Into<RefOr<Parameter>>,
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
    /// * Defaults to [`Required::Unset`], which is omitted from the serialized output.
    #[serde(default, skip_serializing_if = "is_required_unset")]
    pub required: Required,

    /// Declares the parameter deprecated status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Deprecated>,

    /// Sets the ability to pass empty-valued parameters. Only applicable to
    /// [`ParameterIn::Query`] parameters. Defaults to `false`.
    ///
    /// Note: per the OpenAPI 3.1 spec the use of this property is discouraged and may be
    /// removed in a future version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_empty_value: Option<bool>,

    /// Schema of the parameter. Typically [`Schema::Object`] is used.
    ///
    /// Mutually exclusive with [`Parameter::content`]; per the spec a parameter must
    /// describe its value with one or the other, not both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<RefOr<Schema>>,

    /// Describes how [`Parameter`] is being serialized depending on [`Parameter::schema`] (type of
    /// a content). Default value is based on [`ParameterIn`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<ParameterStyle>,

    /// When _`true`_ it will generate separate parameter value for each parameter with _`array`_
    /// and _`object`_ type. This is also _`true`_ by default for [`ParameterStyle::Form`].
    ///
    /// With explode _`false`_:
    /// ```text
    /// color=blue,black,brown
    /// ```
    ///
    /// With explode _`true`_:
    /// ```text
    /// color=blue&color=black&color=brown
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explode: Option<bool>,

    /// When _`true`_, values are serialized using reserved expansion, letting the reserved
    /// characters defined by [RFC3986](https://tools.ietf.org/html/rfc3986#section-2.2)
    /// _`:/?#[]@!$&'()*+,;=`_ and percent-encoded triples pass through unchanged.
    /// Default value is _`false`_.
    ///
    /// OpenAPI 3.1 restricted this to [`ParameterIn::Query`]; OpenAPI 3.2 defines it in terms of
    /// RFC6570 reserved expansion, so it also applies to [`ParameterIn::Path`] parameters using
    /// an RFC6570-based style.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_reserved: Option<bool>,

    /// Example of the [`Parameter`]'s potential value. This example will override any
    /// example defined within [`Parameter::schema`].
    #[serde(skip_serializing_if = "Option::is_none")]
    example: Option<Value>,

    /// Examples of the parameter's potential value, indexed by name. When both
    /// [`Parameter::example`] and `examples` are present, `examples` takes precedence.
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub examples: PropMap<String, RefOr<Example>>,

    /// A map containing the representations for the parameter, keyed by media type.
    ///
    /// Per the OpenAPI 3.1 spec the map must contain exactly one entry. Mutually exclusive
    /// with [`Parameter::schema`].
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub content: PropMap<String, Content>,

    /// Optional extensions "x-something"
    #[serde(skip_serializing_if = "PropMap::is_empty", flatten)]
    pub extensions: PropMap<String, serde_json::Value>,
}

impl Parameter {
    /// Constructs a new required [`Parameter`] with given name.
    #[must_use]
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            required: Required::Unset,
            ..Default::default()
        }
    }
    /// Add name of the [`Parameter`].
    #[must_use]
    pub fn name<I: Into<String>>(mut self, name: I) -> Self {
        self.name = name.into();
        self
    }

    /// Sets the location (`in`) of the [`Parameter`].
    ///
    /// If the location is [`ParameterIn::Path`], the parameter is also marked required.
    #[must_use]
    pub fn location(mut self, location: ParameterIn) -> Self {
        self.parameter_in = location;
        if self.parameter_in == ParameterIn::Path {
            self.required = Required::True;
        }
        self
    }

    /// Sets the location (`in`) of the [`Parameter`].
    #[deprecated(since = "0.94.0", note = "use `Parameter::location` instead")]
    #[must_use]
    pub fn parameter_in(self, parameter_in: ParameterIn) -> Self {
        self.location(parameter_in)
    }

    /// Merge values from another [`Parameter`] with the same name and location.
    ///
    /// Fields supplied by `other` overwrite the corresponding fields in `self`.
    /// A supplied schema or nonempty content map replaces the complete value representation,
    /// keeping `schema` and `content` mutually exclusive. When neither is supplied, the
    /// existing representation is preserved. Returns `false` without changing `self` if
    /// the name or location differs.
    pub fn merge(&mut self, other: Self) -> bool {
        let Self {
            name,
            parameter_in,
            description,
            required,
            deprecated,
            allow_empty_value,
            schema,
            style,
            explode,
            allow_reserved,
            example,
            examples,
            content,
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
        // Per the OpenAPI 3.1 spec, path parameters MUST have `required: true`.
        if self.parameter_in == ParameterIn::Path {
            self.required = Required::True;
        }

        if let Some(deprecated) = deprecated {
            self.deprecated = Some(deprecated);
        }
        if let Some(allow_empty_value) = allow_empty_value {
            self.allow_empty_value = Some(allow_empty_value);
        }
        if !content.is_empty() {
            self.schema = None;
            self.content = content;
        } else if let Some(schema) = schema {
            self.schema = Some(schema);
            self.content.clear();
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
        for (k, v) in examples {
            self.examples.insert(k, v);
        }
        self.extensions.extend(extensions);
        true
    }

    /// Add required declaration of the [`Parameter`]. If [`ParameterIn::Path`] is
    /// defined this is always [`Required::True`].
    #[must_use]
    pub fn required(mut self, required: impl Into<Required>) -> Self {
        self.required = required.into();
        // required must be true, if parameter_in is Path
        if self.parameter_in == ParameterIn::Path {
            self.required = Required::True;
        }

        self
    }

    /// Add or change description of the [`Parameter`].
    #[must_use]
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add or change [`Parameter`] deprecated declaration.
    #[must_use]
    pub fn deprecated<D: Into<Deprecated>>(mut self, deprecated: D) -> Self {
        self.deprecated = Some(deprecated.into());
        self
    }

    /// Add or change [`Parameter`]s schema.
    ///
    /// Clears any previously configured [`Parameter::content`].
    #[must_use]
    pub fn schema<I: Into<RefOr<Schema>>>(mut self, component: I) -> Self {
        self.schema = Some(component.into());
        self.content.clear();
        self
    }

    /// Add or change serialization style of [`Parameter`].
    #[must_use]
    pub fn style(mut self, style: ParameterStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Define whether [`Parameter`]s are exploded or not.
    #[must_use]
    pub fn explode(mut self, explode: bool) -> Self {
        self.explode = Some(explode);
        self
    }

    /// Add or change whether [`Parameter`] should allow reserved characters.
    #[must_use]
    pub fn allow_reserved(mut self, allow_reserved: bool) -> Self {
        self.allow_reserved = Some(allow_reserved);
        self
    }

    /// Add or change example of [`Parameter`]'s potential value.
    #[must_use]
    pub fn example(mut self, example: Value) -> Self {
        self.example = Some(example);
        self
    }

    /// Insert a named [`Example`] (or a [`Ref`](crate::Ref) to one) into [`Parameter::examples`].
    ///
    /// When set, `examples` takes precedence over [`Parameter::example`].
    #[must_use]
    pub fn add_example<N: Into<String>, E: Into<RefOr<Example>>>(
        mut self,
        name: N,
        example: E,
    ) -> Self {
        self.examples.insert(name.into(), example.into());
        self
    }

    /// Replace [`Parameter::examples`] with the contents of an iterator of named examples.
    #[must_use]
    pub fn examples<I, N, E>(mut self, examples: I) -> Self
    where
        I: IntoIterator<Item = (N, E)>,
        N: Into<String>,
        E: Into<RefOr<Example>>,
    {
        self.examples = examples
            .into_iter()
            .map(|(name, example)| (name.into(), example.into()))
            .collect();
        self
    }

    /// Set a single media-type entry in [`Parameter::content`].
    ///
    /// Per spec the `content` map must contain exactly one entry. Mutually exclusive with
    /// [`Parameter::schema`]. Replaces previously configured content and clears the schema.
    #[must_use]
    pub fn content<S: Into<String>, C: Into<Content>>(mut self, media_type: S, content: C) -> Self {
        self.schema = None;
        self.content = PropMap::from([(media_type.into(), content.into())]);
        self
    }

    /// Allow or disallow empty-valued query parameters.
    ///
    /// Note: per the OpenAPI 3.1 spec the use of this property is discouraged.
    #[must_use]
    pub fn allow_empty_value(mut self, allow: bool) -> Self {
        self.allow_empty_value = Some(allow);
        self
    }
}

/// Possible values for the OpenAPI parameter `in` field, indicating where the parameter
/// is located in the request.
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Default, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ParameterIn {
    /// Declares that parameter is used as query parameter.
    ///
    /// Must not appear in the same operation as a [`ParameterIn::QueryString`] parameter.
    Query,
    /// Declares that the parameter is the entire URL query string, treated as a single value.
    /// Added in OpenAPI 3.2.
    ///
    /// Such a parameter must describe its value with [`Parameter::content`] rather than
    /// [`Parameter::schema`], must not appear more than once, and must not be combined with any
    /// [`ParameterIn::Query`] parameter in the same operation.
    ///
    /// See <https://spec.openapis.org/oas/v3.2.0.html#parameter-locations>.
    #[serde(rename = "querystring")]
    QueryString,
    /// Declares that parameter is used as path parameter.
    #[default]
    Path,
    /// Declares that parameter is used as header value.
    Header,
    /// Declares that parameter is used as cookie value.
    Cookie,
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
    /// Cookie style parameters as defined by [RFC6265](https://www.rfc-editor.org/rfc/rfc6265),
    /// e.g. _`name=value; other=thing`_. Added in OpenAPI 3.2 and allowed only with
    /// [`ParameterIn::Cookie`].
    ///
    /// See <https://spec.openapis.org/oas/v3.2.0.html#style-values>.
    Cookie,
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;
    use crate::Object;

    #[test]
    fn test_build_parameter() {
        let parameter = Parameter::new("name");
        assert_eq!(parameter.name, "name");

        let parameter = parameter
            .name("new name")
            .location(ParameterIn::Query)
            .required(Required::True)
            .description("description")
            .deprecated(Deprecated::False)
            .schema(Schema::object(Object::new()))
            .style(ParameterStyle::Simple)
            .explode(true)
            .allow_reserved(true)
            .example(Value::String("example".to_owned()));
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
            .example(Value::String("example".to_owned()));

        parameter1.extensions =
            PropMap::from([("key1".to_owned(), Value::String("value1".to_owned()))]);
        parameter2.extensions =
            PropMap::from([("key2".to_owned(), Value::String("value2".to_owned()))]);

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
            .example(Value::String("example".to_owned()));

        parameter2.extensions =
            PropMap::from([("key2".to_owned(), Value::String("value2".to_owned()))]);

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
    fn parameter_in_querystring_round_trips() {
        let parameter = Parameter::new("query")
            .location(ParameterIn::QueryString)
            .content(
                "application/x-www-form-urlencoded",
                crate::Content::new(Schema::object(Object::new())),
            );

        let value = serde_json::to_value(&parameter).expect("serialize");
        assert_eq!(value["in"], json!("querystring"));

        let parsed: Parameter = serde_json::from_value(value).expect("deserialize");
        assert_eq!(parsed.parameter_in, ParameterIn::QueryString);
    }

    #[test]
    fn parameter_style_cookie_round_trips() {
        let parameter = Parameter::new("session")
            .location(ParameterIn::Cookie)
            .style(ParameterStyle::Cookie);

        let value = serde_json::to_value(&parameter).expect("serialize");
        assert_eq!(value["style"], json!("cookie"));

        let parsed: Parameter = serde_json::from_value(value).expect("deserialize");
        assert_eq!(parsed.style, Some(ParameterStyle::Cookie));
    }

    #[test]
    fn querystring_and_query_parameters_are_distinct_entries() {
        let mut parameters = Parameters::new();
        parameters.insert(Parameter::new("q").location(ParameterIn::Query));
        parameters.insert(Parameter::new("q").location(ParameterIn::QueryString));

        assert!(parameters.contains("q", ParameterIn::Query));
        assert!(parameters.contains("q", ParameterIn::QueryString));
        assert_eq!(parameters.0.len(), 2);
    }

    #[test]
    fn test_parameters_into_iter() {
        let parameters = Parameters::new().parameter(Parameter::new("param"));
        let mut iter = parameters.into_iter();
        // `Parameters::insert` forces `required: True` for path parameters per spec, so the
        // round-tripped item is not equal to a fresh `Parameter::new("param")` with
        // `Required::Unset`.
        assert_eq!(
            iter.next(),
            Some(RefOr::Type(
                Parameter::new("param").required(Required::True)
            ))
        );
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
                    "required": true
                },
                {
                    "in": "path",
                    "name": "param2",
                    "required": true
                }
            ])
        );
    }

    #[test]
    fn parameter_required_unset_is_omitted() {
        let parameter = Parameter::new("filter").location(ParameterIn::Query);
        let value = serde_json::to_value(&parameter).expect("serialize");
        assert!(
            value.get("required").is_none(),
            "expected `required` to be omitted when Unset; got: {value}"
        );
    }

    #[test]
    fn parameter_required_true_is_emitted_for_path_in() {
        let parameter = Parameter::new("id").location(ParameterIn::Path);
        // parameter_in(Path) should force required=true (per spec) and serialize it.
        let value = serde_json::to_value(&parameter).expect("serialize");
        assert_eq!(value["required"], serde_json::Value::Bool(true));
    }

    #[test]
    fn parameter_with_examples_serializes_under_camel_case_field() {
        use crate::Example;

        let parameter = Parameter::new("filter")
            .location(ParameterIn::Query)
            .add_example("first", Example::new().value(json!("foo")))
            .add_example("second", Example::new().value(json!("bar")));

        let value = serde_json::to_value(&parameter).expect("serialize");
        assert_eq!(
            value["examples"],
            json!({
                "first":  { "value": "foo" },
                "second": { "value": "bar" }
            })
        );
    }

    #[test]
    fn parameter_with_content_serializes_as_media_type_map() {
        let parameter = Parameter::new("filter")
            .location(ParameterIn::Query)
            .content(
                "application/json",
                Content::new(RefOr::Ref(crate::Ref::from_schema_name("Filter"))),
            );

        let value = serde_json::to_value(&parameter).expect("serialize");
        assert_eq!(
            value["content"],
            json!({
                "application/json": {
                    "schema": { "$ref": "#/components/schemas/Filter" }
                }
            })
        );
    }

    #[test]
    fn parameter_allow_empty_value_round_trips_under_camel_case() {
        let parameter = Parameter::new("flag")
            .location(ParameterIn::Query)
            .allow_empty_value(true);

        let value = serde_json::to_value(&parameter).expect("serialize");
        assert_eq!(value["allowEmptyValue"], serde_json::Value::Bool(true));
    }
}
