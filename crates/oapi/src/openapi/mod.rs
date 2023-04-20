//! Rust implementation of Openapi Spec V3.

use serde::{de::Visitor, Deserialize, Serialize, Serializer};

pub use self::{
    content::Content,
    external_docs::ExternalDocs,
    header::Header,
    info::{Contact, Info, License},
    operation::Operation,
    parameter::{Parameter, ParameterIn, ParameterStyle},
    path::{PathItem, PathItemType, Paths},
    response::{Response, Responses},
    schema::{Array, Components, Discriminator, KnownFormat, Object, Ref, Schema, SchemaFormat, SchemaType, ToArray},
    security::{SecurityRequirement, SecurityScheme},
    server::{Server, ServerVariable},
    tag::Tag,
    xml::Xml,
};

mod content;
mod encoding;
mod example;
mod external_docs;
mod header;
pub mod info;
pub mod operation;
pub mod parameter;
pub mod path;
mod request_body;
pub mod response;
pub mod schema;
pub mod security;
pub mod server;
mod tag;
mod xml;

/// Root object of the OpenAPI document.
///
/// You can use [`OpenApi::new`] function to construct a new [`OpenApi`] instance and then
/// use the fields with mutable access to modify them. This is quite tedious if you are not simply
/// just changing one thing thus you can also use the [`OpenApi::new`] to use builder to
/// construct a new [`OpenApi`] object.
///
/// See more details at <https://spec.openapis.org/oas/latest.html#openapi-object>.
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OpenApi {
    /// OpenAPI document version.
    pub openapi: OpenApiVersion,

    /// Provides metadata about the API.
    ///
    /// See more details at <https://spec.openapis.org/oas/latest.html#info-object>.
    pub info: Info,

    /// Optional list of servers that provides the connectivity information to target servers.
    ///
    /// This is implicitly one server with `url` set to `/`.
    ///
    /// See more details at <https://spec.openapis.org/oas/latest.html#server-object>.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<Server>>,

    /// Available paths and operations for the API.
    ///
    /// See more details at <https://spec.openapis.org/oas/latest.html#paths-object>.
    #[serde(flatten)]
    pub paths: Paths,

    /// Holds various reusable schemas for the OpenAPI document.
    ///
    /// Few of these elements are security schemas and object schemas.
    ///
    /// See more details at <https://spec.openapis.org/oas/latest.html#components-object>.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,

    /// Declaration of global security mechanisms that can be used across the API. The individual operations
    /// can override the declarations. You can use `SecurityRequirement::default()` if you wish to make security
    /// optional by adding it to the list of securities.
    ///
    /// See more details at <https://spec.openapis.org/oas/latest.html#security-requirement-object>.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<Vec<SecurityRequirement>>,

    /// Optional list of tags can be used to add additional documentation to matching tags of operations.
    ///
    /// See more details at <https://spec.openapis.org/oas/latest.html#tag-object>.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<Tag>>,

    /// Optional global additional documentation reference.
    ///
    /// See more details at <https://spec.openapis.org/oas/latest.html#external-documentation-object>.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_docs: Option<ExternalDocs>,
}

impl OpenApi {
    /// Construct a new [`OpenApi`] object.
    ///
    /// Function accepts two arguments one which is [`Info`] metadata of the API; two which is [`Paths`]
    /// containing operations for the API.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_oapi::{Info, Paths, OpenApi};
    /// #
    /// let openapi = OpenApi::new(Info::new("pet api", "0.1.0"), Paths::new());
    /// ```
    pub fn new(info: Info) -> Self {
        Self {
            info,
            ..Default::default()
        }
    }

    /// Converts this [`OpenApi`] to JSON String. This method essentially calls [`serde_json::to_string`] method.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Converts this [`OpenApi`] to pretty JSON String. This method essentially calls [`serde_json::to_string_pretty`] method.
    pub fn to_pretty_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Converts this [`OpenApi`] to YAML String. This method essentially calls [`serde_yaml::to_string`] method.
    #[cfg(feature = "yaml")]
    #[cfg_attr(doc_cfg, doc(cfg(feature = "yaml")))]
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
    }

    /// Merge `other` [`OpenApi`] consuming it and resuming it's content.
    ///
    /// Merge function will take all `self` nonexistent _`servers`, `paths`, `schemas`, `responses`,
    /// `security_schemes`, `security_requirements` and `tags`_ from _`other`_ [`OpenApi`].
    ///
    /// This function performs a shallow comparison for `paths`, `schemas`, `responses` and
    /// `security schemes` which means that only _`name`_ and _`path`_ is used for comparison. When
    /// match occurs the whole item will be ignored from merged results. Only items not
    /// found will be appended to `self`.
    ///
    /// For _`servers`_, _`tags`_ and _`security_requirements`_ the whole item will be used for
    /// comparison. Items not found from `self` will be appended to `self`.
    ///
    /// **Note!** `info`, `openapi` and `external_docs` will not be merged.
    pub fn merge(&mut self, mut other: OpenApi) {
        if let Some(other_servers) = &mut other.servers {
            let servers = self.servers.get_or_insert(Vec::new());
            other_servers.retain(|server| !servers.contains(server));
            servers.append(other_servers);
        }

        if !other.paths.paths.is_empty() {
            other
                .paths
                .paths
                .retain(|path, _| self.paths.get_path_item(path).is_none());
            self.paths.paths.append(&mut other.paths.paths);
        };

        if let Some(other_components) = &mut other.components {
            let components = self.components.get_or_insert(Components::default());

            other_components
                .schemas
                .retain(|name, _| !components.schemas.contains_key(name));
            components.schemas.append(&mut other_components.schemas);

            other_components
                .responses
                .retain(|name, _| !components.responses.contains_key(name));
            components.responses.append(&mut other_components.responses);

            other_components
                .security_schemes
                .retain(|name, _| !components.security_schemes.contains_key(name));
            components
                .security_schemes
                .append(&mut other_components.security_schemes);
        }

        if let Some(other_security) = &mut other.security {
            let security = self.security.get_or_insert(Vec::new());
            other_security.retain(|requirement| !security.contains(requirement));
            security.append(other_security);
        }

        if let Some(other_tags) = &mut other.tags {
            let tags = self.tags.get_or_insert(Vec::new());
            other_tags.retain(|tag| !tags.contains(tag));
            tags.append(other_tags);
        }
    }

    /// Add [`Info`] metadata of the API.
    pub fn info<I: Into<Info>>(mut self, info: I) -> Self {
        set_value!(self info info.into())
    }

    /// Add iterator of [`Server`]s to configure target servers.
    pub fn servers<S: IntoIterator<Item = Server>>(mut self, servers: S) -> Self {
        set_value!(self servers Some(servers.into_iter().collect()))
    }

    /// Add [`Paths`] to configure operations and endpoints of the API.
    pub fn paths(mut self, paths: impl Into<Paths>) -> Self {
        set_value!(self paths paths.into())
    }

    /// Add [`Components`] to configure reusable schemas.
    pub fn components(mut self, components: impl Into<Components>) -> Self {
        set_value!(self components Some(components.into()))
    }

    /// Add iterator of [`SecurityRequirement`]s that are globally available for all operations.
    pub fn security<S: IntoIterator<Item = SecurityRequirement>>(mut self, security: S) -> Self {
        set_value!(self security Some(security.into_iter().collect()))
    }

    /// Add iterator of [`Tag`]s to add additional documentation for **operations** tags.
    pub fn tags<I: IntoIterator<Item = Tag>>(mut self, tags: I) -> Self {
        set_value!(self tags Some(tags.into_iter().collect()))
    }

    /// Add [`ExternalDocs`] for referring additional documentation.
    pub fn external_docs(mut self, external_docs: ExternalDocs) -> Self {
        set_value!(self external_docs Some(external_docs))
    }
}

/// Represents available [OpenAPI versions][version].
///
/// [version]: <https://spec.openapis.org/oas/latest.html#versions>
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub enum OpenApiVersion {
    /// Will serialize to `3.0.3` the latest from 3.0 serde.
    #[serde(rename = "3.0.3")]
    Version3,
}

impl Default for OpenApiVersion {
    fn default() -> Self {
        Self::Version3
    }
}

/// Value used to indicate whether reusable schema, parameter or operation is deprecated.
///
/// The value will serialize to boolean.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Deprecated {
    True,
    False,
}

impl Serialize for Deprecated {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bool(matches!(self, Self::True))
    }
}

impl<'de> Deserialize<'de> for Deprecated {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct BoolVisitor;
        impl<'de> Visitor<'de> for BoolVisitor {
            type Value = Deprecated;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a bool true or false")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    true => Ok(Deprecated::True),
                    false => Ok(Deprecated::False),
                }
            }
        }
        deserializer.deserialize_bool(BoolVisitor)
    }
}

impl Default for Deprecated {
    fn default() -> Self {
        Deprecated::False
    }
}

/// Value used to indicate whether parameter or property is required.
///
/// The value will serialize to boolean.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Required {
    True,
    False,
}

impl Serialize for Required {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bool(matches!(self, Self::True))
    }
}

impl<'de> Deserialize<'de> for Required {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct BoolVisitor;
        impl<'de> Visitor<'de> for BoolVisitor {
            type Value = Required;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a bool true or false")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    true => Ok(Required::True),
                    false => Ok(Required::False),
                }
            }
        }
        deserializer.deserialize_bool(BoolVisitor)
    }
}

impl Default for Required {
    fn default() -> Self {
        Required::False
    }
}

/// A [`Ref`] or some other type `T`.
///
/// Typically used in combination with [`Components`] and is an union type between [`Ref`] and any
/// other given type such as [`Schema`] or [`Response`].
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(untagged)]
pub enum RefOr<T> {
    Ref(schema::Ref),
    T(T),
}

macro_rules! set_value {
    ( $self:ident $field:ident $value:expr ) => {{
        $self.$field = $value;
        $self
    }};
}
pub(crate) use set_value;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{
        info::Info,
        path::{Operation, Paths},
    };

    use super::{response::Response, *};

    #[test]
    fn serialize_deserialize_openapi_version_success() -> Result<(), serde_json::Error> {
        assert_eq!(serde_json::to_value(&OpenApiVersion::Version3)?, "3.0.3");
        Ok(())
    }

    #[test]
    fn serialize_openapi_json_minimal_success() -> Result<(), serde_json::Error> {
        let raw_json = include_str!("openapi/testdata/expected_openapi_minimal.json");
        let openapi = OpenApi::new(
            Info::new("My api", "1.0.0")
                .description("My api description")
                .license(License::new("MIT").url(Some("http://mit.licence"))),
            Paths::new(),
        );
        let serialized = serde_json::to_string_pretty(&openapi)?;

        assert_eq!(
            serialized, raw_json,
            "expected serialized json to match raw: \nserialized: \n{serialized} \nraw: \n{raw_json}"
        );
        Ok(())
    }

    #[test]
    fn serialize_openapi_json_with_paths_success() -> Result<(), serde_json::Error> {
        let openapi = OpenApi::new(
            Info::new("My big api", "1.1.0"),
            Paths::new()
                .path(
                    "/api/v1/users",
                    PathItem::new(
                        PathItemType::Get,
                        Operation::new().response("200", Response::new("Get users list")),
                    ),
                )
                .path(
                    "/api/v1/users",
                    PathItem::new(
                        PathItemType::Post,
                        Operation::new().response("200", Response::new("Post new user")),
                    ),
                )
                .path(
                    "/api/v1/users/{id}",
                    PathItem::new(
                        PathItemType::Get,
                        Operation::new().response("200", Response::new("Get user by id")),
                    ),
                ),
        );

        let serialized = serde_json::to_string_pretty(&openapi)?;
        let expected = include_str!("./openapi/testdata/expected_openapi_with_paths.json");

        assert_eq!(
            serialized, expected,
            "expected serialized json to match raw: \nserialized: \n{serialized} \nraw: \n{expected}"
        );
        Ok(())
    }

    #[test]
    fn merge_2_openapi_documents() {
        let mut api_1 = OpenApi::new(
            Info::new("Api", "v1"),
            Paths::new()
                .path(
                    "/api/v1/user",
                    PathItem::new(
                        PathItemType::Get,
                        Operation::new().response("200", Response::new("Get user success")),
                    ),
                )
                .build(),
        );

        let api_2 = OpenApi::new()
            .info(Info::new("Api", "v2"))
            .paths(
                Paths::new()
                    .path(
                        "/api/v1/user",
                        PathItem::new(
                            PathItemType::Get,
                            Operation::new().response("200", Response::new("This will not get added")),
                        ),
                    )
                    .path(
                        "/ap/v2/user",
                        PathItem::new(
                            PathItemType::Get,
                            Operation::new().response("200", Response::new("Get user success 2")),
                        ),
                    )
                    .path(
                        "/api/v2/user",
                        PathItem::new(
                            PathItemType::Post,
                            Operation::new().response("200", Response::new("Get user success")),
                        ),
                    )
                    .build(),
            )
            .components(Some(
                Components::new()
                    .schema(
                        "User2",
                        Object::new()
                            .schema_type(SchemaType::Object)
                            .property("name", Object::new().schema_type(SchemaType::String)),
                    )
                    .build(),
            ))
            .build();

        api_1.merge(api_2);
        let value = serde_json::to_value(&api_1).unwrap();

        assert_eq!(
            value,
            json!(
                {
                  "openapi": "3.0.3",
                  "info": {
                    "title": "Api",
                    "version": "v1"
                  },
                  "paths": {
                    "/ap/v2/user": {
                      "get": {
                        "responses": {
                          "200": {
                            "description": "Get user success 2"
                          }
                        }
                      }
                    },
                    "/api/v1/user": {
                      "get": {
                        "responses": {
                          "200": {
                            "description": "Get user success"
                          }
                        }
                      }
                    },
                    "/api/v2/user": {
                      "post": {
                        "responses": {
                          "200": {
                            "description": "Get user success"
                          }
                        }
                      }
                    }
                  },
                  "components": {
                    "schemas": {
                      "User2": {
                        "type": "object",
                        "properties": {
                          "name": {
                            "type": "string"
                          }
                        }
                      }
                    }
                  }
                }
            )
        )
    }
}
