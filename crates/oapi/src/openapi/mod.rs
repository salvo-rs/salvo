//! Rust implementation of Openapi Spec V3.
use std::collections::{btree_map, BTreeSet};

use salvo_core::{async_trait, writer, Depot, FlowCtrl, Handler, Router};
use serde::{de::Visitor, Deserialize, Serialize, Serializer};

pub use self::{
    components::Components,
    content::Content,
    example::Example,
    external_docs::ExternalDocs,
    header::Header,
    info::{Contact, Info, License},
    operation::{Operation, Operations},
    parameter::{Parameter, ParameterIn, ParameterStyle, Parameters},
    path::{PathItem, PathItemType, Paths},
    request_body::RequestBody,
    response::{Response, Responses},
    schema::{Array, Discriminator, KnownFormat, Object, Ref, Schema, SchemaFormat, SchemaType, ToArray},
    security::{SecurityRequirement, SecurityScheme},
    server::{Server, ServerVariable, ServerVariables, Servers},
    tag::Tag,
    xml::Xml,
};

mod components;
mod content;
mod encoding;
mod example;
mod external_docs;
mod header;
pub mod info;
pub mod operation;
pub mod parameter;
pub mod path;
pub mod request_body;
pub mod response;
pub mod schema;
pub mod security;
pub mod server;
mod tag;
mod xml;

use crate::{router::NormNode, Endpoint};

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

    /// List of servers that provides the connectivity information to target servers.
    ///
    /// This is implicitly one server with `url` set to `/`.
    ///
    /// See more details at <https://spec.openapis.org/oas/latest.html#server-object>.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub servers: BTreeSet<Server>,

    /// Available paths and operations for the API.
    ///
    /// See more details at <https://spec.openapis.org/oas/latest.html#paths-object>.
    pub paths: Paths,

    /// Holds various reusable schemas for the OpenAPI document.
    ///
    /// Few of these elements are security schemas and object schemas.
    ///
    /// See more details at <https://spec.openapis.org/oas/latest.html#components-object>.
    #[serde(skip_serializing_if = "Components::is_empty")]
    pub components: Components,

    /// Declaration of global security mechanisms that can be used across the API. The individual operations
    /// can override the declarations. You can use `SecurityRequirement::default()` if you wish to make security
    /// optional by adding it to the list of securities.
    ///
    /// See more details at <https://spec.openapis.org/oas/latest.html#security-requirement-object>.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub security: BTreeSet<SecurityRequirement>,

    /// List of tags can be used to add additional documentation to matching tags of operations.
    ///
    /// See more details at <https://spec.openapis.org/oas/latest.html#tag-object>.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub tags: BTreeSet<Tag>,

    /// Global additional documentation reference.
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
    /// let openapi = OpenApi::new(Info::new("pet api", "0.1.0"));
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
    /// match occurs the exists item will be overwrite.
    ///
    /// For _`servers`_, _`tags`_ and _`security_requirements`_ the whole item will be used for
    /// comparison.
    ///
    /// **Note!** `info`, `openapi` and `external_docs` will not be merged.
    pub fn merge(mut self, mut other: OpenApi) -> Self {
        self.servers.append(&mut other.servers);
        self.paths.append(&mut other.paths);
        self.components.append(&mut other.components);
        self.security.append(&mut other.security);
        self.tags.append(&mut other.tags);
        self
    }

    /// Add [`Info`] metadata of the API.
    pub fn info<I: Into<Info>>(mut self, info: I) -> Self {
        set_value!(self info info.into())
    }

    /// Add iterator of [`Server`]s to configure target servers.
    pub fn servers<S: IntoIterator<Item = Server>>(mut self, servers: S) -> Self {
        set_value!(self servers servers.into_iter().collect())
    }

    /// Set paths to configure operations and endpoints of the API.
    pub fn paths<P: Into<Paths>>(mut self, paths: P) -> Self {
        set_value!(self paths paths.into())
    }
    /// Add [`PathItem`] to configure operations and endpoints of the API.
    pub fn add_path<P, I>(mut self, path: P, item: I) -> Self
    where
        P: Into<String>,
        I: Into<PathItem>,
    {
        self.paths.insert(path.into(), item.into());
        self
    }

    /// Add [`Components`] to configure reusable schemas.
    pub fn components(mut self, components: impl Into<Components>) -> Self {
        set_value!(self components components.into())
    }

    /// Add iterator of [`SecurityRequirement`]s that are globally available for all operations.
    pub fn security<S: IntoIterator<Item = SecurityRequirement>>(mut self, security: S) -> Self {
        set_value!(self security security.into_iter().collect())
    }

    /// Add iterator of [`Tag`]s to add additional documentation for **operations** tags.
    pub fn tags<I: IntoIterator<Item = Tag>>(mut self, tags: I) -> Self {
        set_value!(self tags tags.into_iter().collect())
    }

    /// Add [`ExternalDocs`] for referring additional documentation.
    pub fn external_docs(mut self, external_docs: ExternalDocs) -> Self {
        set_value!(self external_docs Some(external_docs))
    }

    /// Consusmes the [`OpenApi`] and returns [`Router`] with the [`OpenApi`] as handler.
    pub fn into_router(self, path: impl Into<String>) -> Router {
        Router::with_path(path.into()).handle(self)
    }

    /// Consusmes the [`OpenApi`] and informations from a [`Router`].
    pub fn merge_router(self, router: &Router) -> Self {
        self.merge_router_with_base(router, "/")
    }

    /// Consusmes the [`OpenApi`] and informations from a [`Router`] with base path.
    pub fn merge_router_with_base(mut self, router: &Router, base: impl AsRef<str>) -> Self {
        let mut node = NormNode::new(router);
        self.merge_norm_node(&mut node, base.as_ref());
        self
    }

    fn merge_norm_node(&mut self, node: &mut NormNode, base_path: &str) {
        fn join_path(a: &str, b: &str) -> String {
            if a.is_empty() {
                b.to_owned()
            } else if b.is_empty() {
                a.to_owned()
            } else {
                format!("{}/{}", a.trim_end_matches('/'), b.trim_start_matches('/'))
            }
        }

        let path = join_path(base_path, node.path.as_deref().unwrap_or_default());
        if let Some(type_id) = &node.type_id {
            if let Some(creator) = crate::EndpointRegistry::find(type_id) {
                let Endpoint {
                    operation,
                    mut components,
                } = (creator)();
                let methods = if let Some(method) = &node.method {
                    vec![*method]
                } else {
                    vec![
                        PathItemType::Get,
                        PathItemType::Post,
                        PathItemType::Put,
                        PathItemType::Patch,
                    ]
                };
                let path_item = self.paths.entry(path.clone()).or_default();
                for method in methods {
                    if let btree_map::Entry::Vacant(e) = path_item.operations.entry(method) {
                        e.insert(operation.clone());
                    } else {
                        tracing::warn!("path `{}` already contains operation for method `{:?}`", path, method);
                    }
                }
                self.components.append(&mut components);
            }
        }
        for child in &mut node.children {
            self.merge_norm_node(child, &path);
        }
    }
}

#[async_trait]
impl Handler for OpenApi {
    async fn handle(
        &self,
        req: &mut salvo_core::Request,
        _depot: &mut Depot,
        res: &mut salvo_core::Response,
        _ctrl: &mut FlowCtrl,
    ) {
        let pretty = req.queries().get("pretty").map(|v| &**v != "false").unwrap_or(false);
        let content = if pretty {
            self.to_pretty_json().unwrap()
        } else {
            self.to_json().unwrap()
        };
        res.render(writer::Text::Json(&content));
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
    /// Is deprecated.
    True,
    /// Is not deprecated.
    False,
}
impl From<bool> for Deprecated {
    fn from(b: bool) -> Self {
        if b {
            Self::True
        } else {
            Self::False
        }
    }
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

/// Value used to indicate whether parameter or property is required.
///
/// The value will serialize to boolean.
#[derive(PartialEq, Eq, Default, Clone, Debug)]
pub enum Required {
    /// Is required.
    True,
    /// Is not required.
    #[default]
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

/// A [`Ref`] or some other type `T`.
///
/// Typically used in combination with [`Components`] and is an union type between [`Ref`] and any
/// other given type such as [`Schema`] or [`Response`].
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(untagged)]
pub enum RefOr<T> {
    /// A [`Ref`] to a reusable component.
    Ref(schema::Ref),
    /// Some other type `T`.
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

    use crate::{info::Info, Operation, Paths};

    use super::{response::Response, *};

    #[test]
    fn serialize_deserialize_openapi_version_success() -> Result<(), serde_json::Error> {
        assert_eq!(serde_json::to_value(&OpenApiVersion::Version3)?, "3.0.3");
        Ok(())
    }

    #[test]
    fn serialize_openapi_json_minimal_success() -> Result<(), serde_json::Error> {
        let raw_json = include_str!("../../testdata/expected_openapi_minimal.json").replace("\r\n", "\n");
        let openapi = OpenApi::new(
            Info::new("My api", "1.0.0")
                .description("My api description")
                .license(License::new("MIT").url("http://mit.licence")),
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
        let openapi = OpenApi::new(Info::new("My big api", "1.1.0")).paths(
            Paths::new()
                .path(
                    "/api/v1/users",
                    PathItem::new(
                        PathItemType::Get,
                        Operation::new().add_response("200", Response::new("Get users list")),
                    ),
                )
                .path(
                    "/api/v1/users",
                    PathItem::new(
                        PathItemType::Post,
                        Operation::new().add_response("200", Response::new("Post new user")),
                    ),
                )
                .path(
                    "/api/v1/users/{id}",
                    PathItem::new(
                        PathItemType::Get,
                        Operation::new().add_response("200", Response::new("Get user by id")),
                    ),
                ),
        );

        let serialized = serde_json::to_string_pretty(&openapi)?;
        let expected = include_str!("../../testdata/expected_openapi_with_paths.json").replace("\r\n", "\n");

        assert_eq!(
            serialized, expected,
            "expected serialized json to match raw: \nserialized: \n{serialized} \nraw: \n{expected}"
        );
        Ok(())
    }

    #[test]
    fn merge_2_openapi_documents() {
        let mut api_1 = OpenApi::new(Info::new("Api", "v1")).paths(Paths::new().path(
            "/api/v1/user",
            PathItem::new(
                PathItemType::Get,
                Operation::new().add_response("200", Response::new("This will not get added")),
            ),
        ));

        let api_2 = OpenApi::new(Info::new("Api", "v2"))
            .paths(
                Paths::new()
                    .path(
                        "/api/v1/user",
                        PathItem::new(
                            PathItemType::Get,
                            Operation::new().add_response("200", Response::new("Get user success")),
                        ),
                    )
                    .path(
                        "/ap/v2/user",
                        PathItem::new(
                            PathItemType::Get,
                            Operation::new().add_response("200", Response::new("Get user success 2")),
                        ),
                    )
                    .path(
                        "/api/v2/user",
                        PathItem::new(
                            PathItemType::Post,
                            Operation::new().add_response("200", Response::new("Get user success")),
                        ),
                    ),
            )
            .components(
                Components::new().add_schema(
                    "User2",
                    Object::new()
                        .schema_type(SchemaType::Object)
                        .property("name", Object::new().schema_type(SchemaType::String)),
                ),
            );

        api_1 = api_1.merge(api_2);
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
