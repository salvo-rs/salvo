//! Implements [OpenAPI Components Object][components] holding reusable parts of an OpenAPI
//! document.
//!
//! [components]: https://spec.openapis.org/oas/latest.html#components-object
use serde::{Deserialize, Serialize};

use crate::{
    Callback, Example, Header, Link, Parameter, PathItem, PropMap, RefOr, RequestBody, Response,
    Responses, Schema, Schemas, SecurityScheme,
};

/// Implements [OpenAPI Components Object][components] which holds supported
/// reusable objects.
///
/// Components can hold either reusable types themselves or references to other reusable
/// types.
///
/// [components]: https://spec.openapis.org/oas/latest.html#components-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Components {
    /// Map of reusable [OpenAPI Schema Object][schema]s.
    ///
    /// [schema]: https://spec.openapis.org/oas/latest.html#schema-object
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub schemas: Schemas,

    /// Map of reusable response name, to [OpenAPI Response Object][response]s or [OpenAPI
    /// Reference][reference]s to [OpenAPI Response Object][response]s.
    ///
    /// [response]: https://spec.openapis.org/oas/latest.html#response-object
    /// [reference]: https://spec.openapis.org/oas/latest.html#reference-object
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub responses: Responses,

    /// Map of reusable [OpenAPI Parameter Object][parameter]s, indexed by name.
    ///
    /// [parameter]: https://spec.openapis.org/oas/latest.html#parameter-object
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub parameters: PropMap<String, RefOr<Parameter>>,

    /// Map of reusable [OpenAPI Example Object][example]s, indexed by name.
    ///
    /// [example]: https://spec.openapis.org/oas/latest.html#example-object
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub examples: PropMap<String, RefOr<Example>>,

    /// Map of reusable [OpenAPI Request Body Object][request_body]s, indexed by name.
    ///
    /// [request_body]: https://spec.openapis.org/oas/latest.html#request-body-object
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub request_bodies: PropMap<String, RefOr<RequestBody>>,

    /// Map of reusable [OpenAPI Header Object][header]s, indexed by header name.
    ///
    /// [header]: https://spec.openapis.org/oas/latest.html#header-object
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub headers: PropMap<String, RefOr<Header>>,

    /// Map of reusable [OpenAPI Security Scheme Object][security_scheme]s.
    ///
    /// [security_scheme]: https://spec.openapis.org/oas/latest.html#security-scheme-object
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub security_schemes: PropMap<String, SecurityScheme>,

    /// Map of reusable [OpenAPI Link Object][link]s, indexed by name.
    ///
    /// [link]: https://spec.openapis.org/oas/latest.html#link-object
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub links: PropMap<String, RefOr<Link>>,

    /// Map of reusable [OpenAPI Callback Object][callback]s, indexed by name.
    ///
    /// [callback]: https://spec.openapis.org/oas/latest.html#callback-object
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub callbacks: PropMap<String, RefOr<Callback>>,

    /// Map of reusable [OpenAPI Path Item Object][path_item]s. Added in OpenAPI 3.1; entries
    /// here can be referenced from `paths` or `webhooks` via [`RefOr::Ref`].
    ///
    /// [path_item]: https://spec.openapis.org/oas/v3.1.0#path-item-object
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub path_items: PropMap<String, RefOr<PathItem>>,

    /// Optional extensions "x-something"
    #[serde(skip_serializing_if = "PropMap::is_empty", flatten)]
    pub extensions: PropMap<String, serde_json::Value>,
}

impl Components {
    /// Construct a new empty [`Components`]. This is effectively same as calling
    /// [`Components::default`].
    #[must_use]
    pub fn new() -> Self {
        Default::default()
    }

    /// Add [`SecurityScheme`] to [`Components`] and returns `Self`.
    ///
    /// Accepts two arguments where first is the name of the [`SecurityScheme`]. This is later when
    /// referenced by [`SecurityRequirement`][requirement]s. Second parameter is the
    /// [`SecurityScheme`].
    ///
    /// [requirement]: crate::SecurityRequirement
    #[must_use]
    pub fn add_security_scheme<N: Into<String>, S: Into<SecurityScheme>>(
        mut self,
        name: N,
        security_scheme: S,
    ) -> Self {
        self.security_schemes
            .insert(name.into(), security_scheme.into());

        self
    }

    /// Add iterator of [`SecurityScheme`]s to [`Components`].
    ///
    /// Accepts two arguments where first is the name of the [`SecurityScheme`]. This is later when
    /// referenced by [`SecurityRequirement`][requirement]s. Second parameter is the
    /// [`SecurityScheme`].
    ///
    /// [requirement]: crate::SecurityRequirement
    #[must_use]
    pub fn extend_security_schemes<
        I: IntoIterator<Item = (N, S)>,
        N: Into<String>,
        S: Into<SecurityScheme>,
    >(
        mut self,
        schemas: I,
    ) -> Self {
        self.security_schemes.extend(
            schemas
                .into_iter()
                .map(|(name, item)| (name.into(), item.into())),
        );
        self
    }

    /// Add [`Schema`] to [`Components`] and returns `Self`.
    ///
    /// Accepts two arguments where first is name of the schema and second is the schema itself.
    #[must_use]
    pub fn add_schema<S: Into<String>, I: Into<RefOr<Schema>>>(
        mut self,
        name: S,
        schema: I,
    ) -> Self {
        self.schemas.insert(name, schema);
        self
    }

    /// Add [`Schema`]s from iterator.
    ///
    /// # Examples
    /// ```
    /// # use salvo_oapi::{Components, Object, BasicType, Schema};
    /// Components::new().extend_schemas([(
    ///     "Pet",
    ///     Schema::from(
    ///         Object::new()
    ///             .property("name", Object::new().schema_type(BasicType::String))
    ///             .required("name"),
    ///     ),
    /// )]);
    /// ```
    #[must_use]
    pub fn extend_schemas<I, C, S>(mut self, schemas: I) -> Self
    where
        I: IntoIterator<Item = (S, C)>,
        C: Into<RefOr<Schema>>,
        S: Into<String>,
    {
        self.schemas.extend(
            schemas
                .into_iter()
                .map(|(name, schema)| (name.into(), schema.into())),
        );
        self
    }

    /// Add a new response and returns `self`.
    #[must_use]
    pub fn response<S: Into<String>, R: Into<RefOr<Response>>>(
        mut self,
        name: S,
        response: R,
    ) -> Self {
        self.responses.insert(name.into(), response.into());
        self
    }

    /// Extends responses with the contents of an iterator.
    #[must_use]
    pub fn extend_responses<
        I: IntoIterator<Item = (S, R)>,
        S: Into<String>,
        R: Into<RefOr<Response>>,
    >(
        mut self,
        responses: I,
    ) -> Self {
        self.responses.extend(
            responses
                .into_iter()
                .map(|(name, response)| (name.into(), response.into())),
        );
        self
    }

    /// Insert a reusable [`Parameter`] (or a [`Ref`](crate::Ref) to one) and return `self`.
    #[must_use]
    pub fn add_parameter<N: Into<String>, P: Into<RefOr<Parameter>>>(
        mut self,
        name: N,
        parameter: P,
    ) -> Self {
        self.parameters.insert(name.into(), parameter.into());
        self
    }

    /// Insert a reusable [`Example`] (or a [`Ref`](crate::Ref) to one) and return `self`.
    #[must_use]
    pub fn add_example<N: Into<String>, E: Into<RefOr<Example>>>(
        mut self,
        name: N,
        example: E,
    ) -> Self {
        self.examples.insert(name.into(), example.into());
        self
    }

    /// Insert a reusable [`RequestBody`] (or a [`Ref`](crate::Ref) to one) and return `self`.
    #[must_use]
    pub fn add_request_body<N: Into<String>, B: Into<RefOr<RequestBody>>>(
        mut self,
        name: N,
        request_body: B,
    ) -> Self {
        self.request_bodies.insert(name.into(), request_body.into());
        self
    }

    /// Insert a reusable [`Header`] (or a [`Ref`](crate::Ref) to one) and return `self`.
    #[must_use]
    pub fn add_header<N: Into<String>, H: Into<RefOr<Header>>>(
        mut self,
        name: N,
        header: H,
    ) -> Self {
        self.headers.insert(name.into(), header.into());
        self
    }

    /// Insert a reusable [`Link`] (or a [`Ref`](crate::Ref) to one) and return `self`.
    #[must_use]
    pub fn add_link<N: Into<String>, L: Into<RefOr<Link>>>(mut self, name: N, link: L) -> Self {
        self.links.insert(name.into(), link.into());
        self
    }

    /// Insert a reusable [`Callback`] (or a [`Ref`](crate::Ref) to one) and return `self`.
    #[must_use]
    pub fn add_callback<N: Into<String>, C: Into<RefOr<Callback>>>(
        mut self,
        name: N,
        callback: C,
    ) -> Self {
        self.callbacks.insert(name.into(), callback.into());
        self
    }

    /// Insert a reusable [`PathItem`] (or a [`Ref`](crate::Ref) to one) and return `self`.
    ///
    /// Path Item entries in `components.pathItems` were introduced in OpenAPI 3.1 to support
    /// reusable webhooks and shared path operations.
    #[must_use]
    pub fn add_path_item<N: Into<String>, P: Into<RefOr<PathItem>>>(
        mut self,
        name: N,
        path_item: P,
    ) -> Self {
        self.path_items.insert(name.into(), path_item.into());
        self
    }

    /// Moves all elements from `other` into `self`, leaving `other` empty.
    ///
    /// If a key from `other` is already present in `self`, the existing value is kept and
    /// the duplicate from `other` is dropped.
    pub fn append(&mut self, other: &mut Self) {
        other
            .schemas
            .retain(|name, _| !self.schemas.contains_key(name));
        self.schemas.append(&mut other.schemas);

        other
            .responses
            .retain(|name, _| !self.responses.contains_key(name));
        self.responses.append(&mut other.responses);

        other
            .parameters
            .retain(|name, _| !self.parameters.contains_key(name));
        self.parameters.append(&mut other.parameters);

        other
            .examples
            .retain(|name, _| !self.examples.contains_key(name));
        self.examples.append(&mut other.examples);

        other
            .request_bodies
            .retain(|name, _| !self.request_bodies.contains_key(name));
        self.request_bodies.append(&mut other.request_bodies);

        other
            .headers
            .retain(|name, _| !self.headers.contains_key(name));
        self.headers.append(&mut other.headers);

        other
            .security_schemes
            .retain(|name, _| !self.security_schemes.contains_key(name));
        self.security_schemes.append(&mut other.security_schemes);

        other.links.retain(|name, _| !self.links.contains_key(name));
        self.links.append(&mut other.links);

        other
            .callbacks
            .retain(|name, _| !self.callbacks.contains_key(name));
        self.callbacks.append(&mut other.callbacks);

        other
            .path_items
            .retain(|name, _| !self.path_items.contains_key(name));
        self.path_items.append(&mut other.path_items);

        other
            .extensions
            .retain(|name, _| !self.extensions.contains_key(name));
        self.extensions.append(&mut other.extensions);
    }

    /// Add openapi extensions (`x-something`) for [`Components`].
    #[must_use]
    pub fn extensions(mut self, extensions: PropMap<String, serde_json::Value>) -> Self {
        self.extensions = extensions;
        self
    }

    /// Returns `true` if instance contains no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
            && self.responses.is_empty()
            && self.parameters.is_empty()
            && self.examples.is_empty()
            && self.request_bodies.is_empty()
            && self.headers.is_empty()
            && self.security_schemes.is_empty()
            && self.links.is_empty()
            && self.callbacks.is_empty()
            && self.path_items.is_empty()
            && self.extensions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::json;

    use super::*;
    use crate::{Operation, ParameterIn, PathItemType, Ref};

    #[test]
    fn empty_components_serializes_with_no_fields() {
        assert_json_eq!(Components::new(), json!({}));
        assert!(Components::new().is_empty());
    }

    #[test]
    fn each_new_field_serializes_under_spec_name() {
        let components = Components::new()
            .add_parameter(
                "PageParam",
                Parameter::new("page").parameter_in(ParameterIn::Query),
            )
            .add_example(
                "PetExample",
                RefOr::Ref(Ref::new("#/components/examples/UpstreamPet")),
            )
            .add_request_body("PetBody", RequestBody::new())
            .add_header("X-Rate-Limit", Header::default())
            .add_link("GetPetLink", Link::default())
            .add_callback(
                "OrderShipped",
                Callback::new().path(
                    "{$request.body#/callbackUrl}",
                    PathItem::new(PathItemType::Post, Operation::new()),
                ),
            )
            .add_path_item(
                "PingWebhook",
                PathItem::new(PathItemType::Post, Operation::new()),
            );

        let value = serde_json::to_value(&components).expect("serialize");

        assert!(value.get("parameters").is_some(), "expected parameters");
        assert!(value.get("examples").is_some(), "expected examples");
        assert!(
            value.get("requestBodies").is_some(),
            "expected requestBodies (camelCase)"
        );
        assert!(value.get("headers").is_some(), "expected headers");
        assert!(value.get("links").is_some(), "expected links");
        assert!(value.get("callbacks").is_some(), "expected callbacks");
        assert!(
            value.get("pathItems").is_some(),
            "expected pathItems (camelCase, OAS 3.1)"
        );
    }

    #[test]
    fn is_empty_recognizes_each_new_field() {
        // Adding any one of the new component maps should flip is_empty to false.
        assert!(
            !Components::new()
                .add_parameter("p", Parameter::new("q"))
                .is_empty()
        );
        assert!(
            !Components::new()
                .add_example("e", crate::Example::default())
                .is_empty()
        );
        assert!(
            !Components::new()
                .add_request_body("rb", RequestBody::new())
                .is_empty()
        );
        assert!(
            !Components::new()
                .add_header("h", Header::default())
                .is_empty()
        );
        assert!(!Components::new().add_link("l", Link::default()).is_empty());
        assert!(
            !Components::new()
                .add_callback("cb", Callback::new())
                .is_empty()
        );
        assert!(
            !Components::new()
                .add_path_item("pi", PathItem::new(PathItemType::Get, Operation::new()))
                .is_empty()
        );
    }

    #[test]
    fn append_preserves_self_on_key_collision() {
        let mut a = Components::new().add_parameter("dup", Parameter::new("a_param"));
        let mut b = Components::new()
            .add_parameter("dup", Parameter::new("b_param"))
            .add_parameter("only_b", Parameter::new("b_only_param"));

        a.append(&mut b);

        let dup = a.parameters.get("dup").expect("dup retained");
        match dup {
            RefOr::Type(p) => assert_eq!(p.name, "a_param", "self's value should win on collision"),
            RefOr::Ref(_) => panic!("unexpected ref"),
        }
        assert!(a.parameters.contains_key("only_b"));
    }
}
