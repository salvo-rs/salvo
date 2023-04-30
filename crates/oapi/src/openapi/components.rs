//! Implements [OpenAPI Schema Object][schema] types which can be
//! used to define field properties, enum values, array or object types.
//!
//! [schema]: https://spec.openapis.org/oas/latest.html#schema-object
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{RefOr, Response, Responses, Schema, SecurityScheme};

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
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub schemas: BTreeMap<String, RefOr<Schema>>,

    /// Map of reusable response name, to [OpenAPI Response Object][response]s or [OpenAPI
    /// Reference][reference]s to [OpenAPI Response Object][response]s.
    ///
    /// [response]: https://spec.openapis.org/oas/latest.html#response-object
    /// [reference]: https://spec.openapis.org/oas/latest.html#reference-object
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub responses: Responses,

    /// Map of reusable [OpenAPI Security Scheme Object][security_scheme]s.
    ///
    /// [security_scheme]: https://spec.openapis.org/oas/latest.html#security-scheme-object
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub security_schemes: BTreeMap<String, SecurityScheme>,
}

impl Components {
    /// Construct a new empty [`Components`]. This is effectively same as calling [`Components::default`].
    pub fn new() -> Self {
        Default::default()
    }
    /// Add [`SecurityScheme`] to [`Components`]
    ///
    /// Accepts two arguments where first is the name of the [`SecurityScheme`]. This is later when
    /// referenced by [`SecurityRequirement`][requirement]s. Second parameter is the [`SecurityScheme`].
    ///
    /// [requirement]: ../security/struct.SecurityRequirement.html
    pub fn add_security_scheme<N: Into<String>, S: Into<SecurityScheme>>(
        mut self,
        name: N,
        security_schema: S,
    ) -> Self {
        self.security_schemes.insert(name.into(), security_schema.into());
        self
    }

    /// Add iterator of [`SecurityScheme`]s to [`Components`].
    ///
    /// Accepts two arguments where first is the name of the [`SecurityScheme`]. This is later when
    /// referenced by [`SecurityRequirement`][requirement]s. Second parameter is the [`SecurityScheme`].
    ///
    /// [requirement]: ../security/struct.SecurityRequirement.html
    pub fn add_security_schemes_from_iter<I: IntoIterator<Item = (N, S)>, N: Into<String>, S: Into<SecurityScheme>>(
        mut self,
        schemas: I,
    ) -> Self {
        self.security_schemes
            .extend(schemas.into_iter().map(|(name, item)| (name.into(), item.into())));
        self
    }

    /// Add [`Schema`] to [`Components`].
    ///
    /// Accepts two arguments where first is name of the schema and second is the schema itself.
    pub fn add_schema<S: Into<String>, I: Into<RefOr<Schema>>>(mut self, name: S, schema: I) -> Self {
        self.schemas.insert(name.into(), schema.into());
        self
    }
    /// Add [`Schema`]s from iterator.
    ///
    /// # Examples
    /// ```
    /// # use salvo_oapi::{Components, Object, SchemaType, Schema};
    /// Components::new().schemas_from_iter([(
    ///     "Pet",
    ///     Schema::from(
    ///         Object::new()
    ///             .property(
    ///                 "name",
    ///                 Object::new().schema_type(SchemaType::String),
    ///             )
    ///             .required("name")
    ///     ),
    /// )]);
    /// ```
    pub fn schemas_from_iter<I, C, S>(mut self, schemas: I) -> Self
    where
        I: IntoIterator<Item = (S, C)>,
        C: Into<RefOr<Schema>>,
        S: Into<String>,
    {
        self.schemas
            .extend(schemas.into_iter().map(|(name, schema)| (name.into(), schema.into())));
        self
    }

    /// Add a new response and returns `self`.
    pub fn response<S: Into<String>, R: Into<RefOr<Response>>>(mut self, name: S, response: R) -> Self {
        self.responses.insert(name.into(), response.into());
        self
    }

    /// Extends responses with the contents of an iterator.
    pub fn extend_responses<I: IntoIterator<Item = (S, R)>, S: Into<String>, R: Into<RefOr<Response>>>(
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

    /// Add [`SecurityScheme`] to [`Components`].
    ///
    /// Accepts two arguments where first is the name of the [`SecurityScheme`]. This is later when
    /// referenced by [`SecurityRequirement`][requirement]s. Second parameter is the [`SecurityScheme`].
    ///
    /// [requirement]: ../security/struct.SecurityRequirement.html
    pub fn security_scheme<N: Into<String>, S: Into<SecurityScheme>>(mut self, name: N, security_schema: S) -> Self {
        self.security_schemes.insert(name.into(), security_schema.into());

        self
    }

    /// Moves all elements from `other` into `self`, leaving `other` empty.
    ///
    /// If a key from `other` is already present in `self`, the respective
    /// value from `self` will be overwritten with the respective value from `other`.
    pub fn append(&mut self, other: &mut Components) {
        other.schemas.retain(|name, _| !self.schemas.contains_key(name));
        self.schemas.append(&mut other.schemas);

        other.responses.retain(|name, _| !self.responses.contains_key(name));
        self.responses.append(&mut other.responses);

        other
            .security_schemes
            .retain(|name, _| !self.security_schemes.contains_key(name));
        self.security_schemes.append(&mut other.security_schemes);
    }

    /// Returns `true` if instance contains no elements.
    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty() && self.responses.is_empty() && self.security_schemes.is_empty()
    }
}
