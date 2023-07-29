//! Implements [OpenAPI Schema Object][schema] types which can be
//! used to define field properties, enum values, array or object types.
//!
//! [schema]: https://spec.openapis.org/oas/latest.html#schema-object
use serde::{Deserialize, Serialize};

use crate::RefOr;

mod all_of;
mod any_of;
mod array;
mod object;
mod one_of;

pub use all_of::AllOf;
pub use any_of::AnyOf;
pub use array::{Array, ToArray};
pub use object::Object;
pub use one_of::OneOf;

/// Create an _`empty`_ [`Schema`] that serializes to _`null`_.
///
/// Can be used in places where an item can be serialized as `null`. This is used with unit type
/// enum variants and tuple unit types.
pub fn empty() -> Schema {
    Schema::Object(Object::new().nullable(true).default_value(serde_json::Value::Null))
}

/// Is super type for [OpenAPI Schema Object][schemas]. Schema is reusable resource what can be
/// referenced from path operations and other components using [`Ref`].
///
/// [schemas]: https://spec.openapis.org/oas/latest.html#schema-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged, rename_all = "camelCase")]
pub enum Schema {
    /// Defines array schema from another schema. Typically used with
    /// [`Schema::Object`]. Slice and Vec types are translated to [`Schema::Array`] types.
    Array(Array),
    /// Defines object schema. Object is either `object` holding **properties** which are other [`Schema`]s
    /// or can be a field within the [`Object`].
    Object(Object),
    /// Creates a _OneOf_ type [composite Object][composite] schema. This schema
    /// is used to map multiple schemas together where API endpoint could return any of them.
    /// [`Schema::OneOf`] is created form complex enum where enum holds other than unit types.
    ///
    /// [composite]: https://spec.openapis.org/oas/latest.html#components-object
    OneOf(OneOf),

    /// Creates a _AnyOf_ type [composite Object][composite] schema.
    ///
    /// [composite]: https://spec.openapis.org/oas/latest.html#components-object
    AllOf(AllOf),

    /// Creates a _AnyOf_ type [composite Object][composite] schema.
    ///
    /// [composite]: https://spec.openapis.org/oas/latest.html#components-object
    AnyOf(AnyOf),
}

impl Default for Schema {
    fn default() -> Self {
        Schema::Object(Object::default())
    }
}

// impl Schema {
//     pub fn origin_type_id(&self) -> Option<TypeId> {
//         if let Self::Object(o) = self {
//             o.origin_type_id
//         } else {
//             None
//         }
//     }
// }

/// OpenAPI [Discriminator][discriminator] object which can be optionally used together with
/// [`OneOf`] composite object.
///
/// [discriminator]: https://spec.openapis.org/oas/latest.html#discriminator-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Discriminator {
    /// Defines a discriminator property name which must be found within all composite
    /// objects.
    pub property_name: String,
}

impl Discriminator {
    /// Construct a new [`Discriminator`] object with property name.
    ///
    /// # Examples
    ///
    /// Create a new [`Discriminator`] object for `pet_type` property.
    /// ```
    /// # use salvo_oapi::schema::Discriminator;
    /// let discriminator = Discriminator::new("pet_type");
    /// ```
    pub fn new<I: Into<String>>(property_name: I) -> Self {
        Self {
            property_name: property_name.into(),
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// AdditionalProperties is used to define values of map fields of the [`Schema`].
///
/// The value can either be [`RefOr`] or _`bool`_.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum AdditionalProperties<T> {
    /// Use when value type of the map is a known [`Schema`] or [`Ref`] to the [`Schema`].
    RefOr(RefOr<T>),
    /// Use _`AdditionalProperties::FreeForm(true)`_ when any value is allowed in the map.
    FreeForm(bool),
}

impl<T> From<RefOr<T>> for AdditionalProperties<T> {
    fn from(value: RefOr<T>) -> Self {
        Self::RefOr(value)
    }
}

impl From<Object> for AdditionalProperties<Schema> {
    fn from(value: Object) -> Self {
        Self::RefOr(RefOr::T(Schema::Object(value)))
    }
}

impl From<Ref> for AdditionalProperties<Schema> {
    fn from(value: Ref) -> Self {
        Self::RefOr(RefOr::Ref(value))
    }
}

/// Implements [OpenAPI Reference Object][reference] that can be used to reference
/// reusable components such as [`Schema`]s or [`Response`](super::Response)s.
///
/// [reference]: https://spec.openapis.org/oas/latest.html#reference-object
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct Ref {
    /// Reference location of the actual component.
    #[serde(rename = "$ref")]
    pub ref_location: String,
}

impl Ref {
    /// Construct a new [`Ref`] with custom ref location. In most cases this is not necessary
    /// and [`Ref::from_schema_name`] could be used instead.
    pub fn new<I: Into<String>>(ref_location: I) -> Self {
        Self {
            ref_location: ref_location.into(),
        }
    }

    /// Construct a new [`Ref`] from provided schema name. This will create a [`Ref`] that
    /// references the the reusable schemas.
    pub fn from_schema_name<I: Into<String>>(schema_name: I) -> Self {
        Self::new(format!("#/components/schemas/{}", schema_name.into()))
    }

    /// Construct a new [`Ref`] from provided response name. This will create a [`Ref`] that
    /// references the reusable response.
    pub fn from_response_name<I: Into<String>>(response_name: I) -> Self {
        Self::new(format!("#/components/responses/{}", response_name.into()))
    }
}

impl From<Ref> for RefOr<Schema> {
    fn from(r: Ref) -> Self {
        Self::Ref(r)
    }
}

impl<T> From<T> for RefOr<T> {
    fn from(t: T) -> Self {
        Self::T(t)
    }
}

impl Default for RefOr<Schema> {
    fn default() -> Self {
        Self::T(Schema::Object(Object::new()))
    }
}

impl ToArray for RefOr<Schema> {}

/// Represents data type of [`Schema`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SchemaType {
    /// Used with [`Object`]. Objects always have
    /// _schema_type_ [`SchemaType::Object`].
    Object,
    /// Indicates string type of content. Used with [`Object`] on a `string`
    /// field.
    String,
    /// Indicates integer type of content. Used with [`Object`] on a `number`
    /// field.
    Integer,
    /// Indicates floating point number type of content. Used with
    /// [`Object`] on a `number` field.
    Number,
    /// Indicates boolean type of content. Used with [`Object`] on
    /// a `bool` field.
    Boolean,
    /// Used with [`Array`]. Indicates array type of content.
    Array,
}

impl Default for SchemaType {
    fn default() -> Self {
        Self::Object
    }
}

/// Additional format for [`SchemaType`] to fine tune the data type used. If the **format** is not
/// supported by the UI it may default back to [`SchemaType`] alone.
/// Format is an open value, so you can use any formats, even not those defined by the
/// OpenAPI Specification.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase", untagged)]
pub enum SchemaFormat {
    /// Use to define additional detail about the value.
    KnownFormat(KnownFormat),
    /// Can be used to provide additional detail about the value when [`SchemaFormat::KnownFormat`]
    /// is not suitable.
    Custom(String),
}

/// Known schema format modifier property to provide fine detail of the primitive type.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KnownFormat {
    /// 8 bit integer.
    Int8,
    /// 16 bit integer.
    Int16,
    /// 32 bit integer.
    Int32,
    /// 64 bit integer.
    Int64,
    /// 8 bit unsigned integer.
    UInt8,
    /// 16 bit unsigned integer.
    UInt16,
    /// 32 bit unsigned integer.
    UInt32,
    /// 64 bit unsigned integer.
    UInt64,
    /// floating point number.
    Float,
    /// double (floating point) number.
    Double,
    /// base64 encoded chars.
    Byte,
    /// binary data (octet).
    Binary,
    /// ISO-8601 full date [FRC3339](https://xml2rfc.ietf.org/public/rfc/html/rfc3339.html#anchor14).
    Date,
    /// ISO-8601 full date time [FRC3339](https://xml2rfc.ietf.org/public/rfc/html/rfc3339.html#anchor14).
    #[serde(rename = "date-time")]
    DateTime,
    /// Hint to UI to obscure input.
    Password,
    /// Used with [`String`] values to indicate value is in UUID format.
    ///
    /// **uuid** feature need to be enabled.
    #[cfg(feature = "uuid")]
    #[cfg_attr(doc_cfg, doc(cfg(feature = "uuid")))]
    Uuid,
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::{json, Value};

    use super::*;
    use crate::*;

    #[test]
    fn create_schema_serializes_json() -> Result<(), serde_json::Error> {
        let openapi = OpenApi::new("My api", "1.0.0").components(
            Components::new()
                .add_schema("Person", Ref::new("#/components/PersonModel"))
                .add_schema(
                    "Credential",
                    Schema::from(
                        Object::new()
                            .property(
                                "id",
                                Object::new()
                                    .schema_type(SchemaType::Integer)
                                    .format(SchemaFormat::KnownFormat(KnownFormat::Int32))
                                    .description("Id of credential")
                                    .default_value(json!(1i32)),
                            )
                            .property(
                                "name",
                                Object::new()
                                    .schema_type(SchemaType::String)
                                    .description("Name of credential"),
                            )
                            .property(
                                "status",
                                Object::new()
                                    .schema_type(SchemaType::String)
                                    .default_value(json!("Active"))
                                    .description("Credential status")
                                    .enum_values(["Active", "NotActive", "Locked", "Expired"]),
                            )
                            .property("history", Array::new(Ref::from_schema_name("UpdateHistory")))
                            .property("tags", Object::with_type(SchemaType::String).to_array()),
                    ),
                ),
        );

        let serialized = serde_json::to_string_pretty(&openapi)?;
        println!("serialized json:\n {serialized}");

        let value = serde_json::to_value(&openapi)?;
        let credential = get_json_path(&value, "components.schemas.Credential.properties");
        let person = get_json_path(&value, "components.schemas.Person");

        assert!(
            credential.get("id").is_some(),
            "could not find path: components.schemas.Credential.properties.id"
        );
        assert!(
            credential.get("status").is_some(),
            "could not find path: components.schemas.Credential.properties.status"
        );
        assert!(
            credential.get("name").is_some(),
            "could not find path: components.schemas.Credential.properties.name"
        );
        assert!(
            credential.get("history").is_some(),
            "could not find path: components.schemas.Credential.properties.history"
        );
        assert_json_eq!(
            credential.get("id").unwrap_or(&serde_json::value::Value::Null),
            json!({"type":"integer","format":"int32","description":"Id of credential","default":1})
        );
        assert_json_eq!(
            credential.get("name").unwrap_or(&serde_json::value::Value::Null),
            json!({"type":"string","description":"Name of credential"})
        );
        assert_json_eq!(
            credential.get("status").unwrap_or(&serde_json::value::Value::Null),
            json!({"default":"Active","description":"Credential status","enum":["Active","NotActive","Locked","Expired"],"type":"string"})
        );
        assert_json_eq!(
            credential.get("history").unwrap_or(&serde_json::value::Value::Null),
            json!({"items":{"$ref":"#/components/schemas/UpdateHistory"},"type":"array"})
        );
        assert_eq!(person, &json!({"$ref":"#/components/PersonModel"}));

        Ok(())
    }

    // Examples taken from https://spec.openapis.org/oas/latest.html#model-with-map-dictionary-properties
    #[test]
    fn test_property_order() {
        let json_value = Object::new()
            .property(
                "id",
                Object::new()
                    .schema_type(SchemaType::Integer)
                    .format(SchemaFormat::KnownFormat(KnownFormat::Int32))
                    .description("Id of credential")
                    .default_value(json!(1i32)),
            )
            .property(
                "name",
                Object::new()
                    .schema_type(SchemaType::String)
                    .description("Name of credential"),
            )
            .property(
                "status",
                Object::new()
                    .schema_type(SchemaType::String)
                    .default_value(json!("Active"))
                    .description("Credential status")
                    .enum_values(["Active", "NotActive", "Locked", "Expired"]),
            )
            .property("history", Array::new(Ref::from_schema_name("UpdateHistory")))
            .property("tags", Object::with_type(SchemaType::String).to_array());

        #[cfg(not(feature = "preserve_order"))]
        assert_eq!(
            json_value.properties.keys().collect::<Vec<_>>(),
            vec!["history", "id", "name", "status", "tags"]
        );
    }

    // Examples taken from https://spec.openapis.org/oas/latest.html#model-with-map-dictionary-properties
    #[test]
    fn test_additional_properties() {
        let json_value = Object::new().additional_properties(Object::new().schema_type(SchemaType::String));
        assert_json_eq!(
            json_value,
            json!({
                "type": "object",
                "additionalProperties": {
                    "type": "string"
                }
            })
        );

        let json_value = Object::new().additional_properties(Ref::from_schema_name("ComplexModel"));
        assert_json_eq!(
            json_value,
            json!({
                "type": "object",
                "additionalProperties": {
                    "$ref": "#/components/schemas/ComplexModel"
                }
            })
        )
    }

    #[test]
    fn test_object_with_symbol() {
        let json_value = Object::new().symbol("SomeName");
        assert_json_eq!(
            json_value,
            json!({
                "type": "object",
                "symbol": "SomeName"
            })
        );
    }

    #[test]
    fn derive_object_with_example() {
        let expected = r#"{"type":"object","example":{"age":20,"name":"bob the cat"}}"#;
        let json_value = Object::new().example(json!({"age": 20, "name": "bob the cat"}));

        let value_string = serde_json::to_string(&json_value).unwrap();
        assert_eq!(
            value_string, expected,
            "value string != expected string, {value_string} != {expected}"
        );
    }

    fn get_json_path<'a>(value: &'a Value, path: &str) -> &'a Value {
        path.split('.').fold(value, |acc, fragment| {
            acc.get(fragment).unwrap_or(&serde_json::value::Value::Null)
        })
    }

    #[test]
    fn test_array_new() {
        let array = Array::new(
            Object::new().property(
                "id",
                Object::new()
                    .schema_type(SchemaType::Integer)
                    .format(SchemaFormat::KnownFormat(KnownFormat::Int32))
                    .description("Id of credential")
                    .default_value(json!(1i32)),
            ),
        );

        assert!(matches!(array.schema_type, SchemaType::Array));
    }

    #[test]
    fn test_array_builder() {
        let array: Array = Array::new(
            Object::new().property(
                "id",
                Object::new()
                    .schema_type(SchemaType::Integer)
                    .format(SchemaFormat::KnownFormat(KnownFormat::Int32))
                    .description("Id of credential")
                    .default_value(json!(1i32)),
            ),
        );

        assert!(matches!(array.schema_type, SchemaType::Array));
    }

    #[test]
    fn reserialize_deserialized_schema_components() {
        let components = Components::new()
            .schemas_from_iter(vec![(
                "Comp",
                Schema::from(
                    Object::new()
                        .property("name", Object::new().schema_type(SchemaType::String))
                        .required("name"),
                ),
            )])
            .extend_responses(vec![("200", Response::new("Okay"))])
            .security_scheme("TLS", SecurityScheme::MutualTls { description: None });

        let serialized_components = serde_json::to_string(&components).unwrap();

        let deserialized_components: Components = serde_json::from_str(serialized_components.as_str()).unwrap();

        assert_eq!(
            serialized_components,
            serde_json::to_string(&deserialized_components).unwrap()
        )
    }

    #[test]
    fn reserialize_deserialized_object_component() {
        let prop = Object::new()
            .property("name", Object::new().schema_type(SchemaType::String))
            .required("name");

        let serialized_components = serde_json::to_string(&prop).unwrap();
        let deserialized_components: Object = serde_json::from_str(serialized_components.as_str()).unwrap();

        assert_eq!(
            serialized_components,
            serde_json::to_string(&deserialized_components).unwrap()
        )
    }

    #[test]
    fn reserialize_deserialized_property() {
        let prop = Object::new().schema_type(SchemaType::String);

        let serialized_components = serde_json::to_string(&prop).unwrap();
        let deserialized_components: Object = serde_json::from_str(serialized_components.as_str()).unwrap();

        assert_eq!(
            serialized_components,
            serde_json::to_string(&deserialized_components).unwrap()
        )
    }

    #[test]
    fn serialize_deserialize_array_within_ref_or_t_object_builder() {
        let ref_or_schema = RefOr::T(Schema::Object(Object::new().property(
            "test",
            RefOr::T(Schema::Array(Array::new(RefOr::T(Schema::Object(
                Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
            ))))),
        )));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");

        let json_de_str = serde_json::to_string(&deserialized).expect("");
        println!("----------------------------");
        println!("{json_de_str}");

        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_one_of_within_ref_or_t_object_builder() {
        let ref_or_schema = RefOr::T(Schema::Object(
            Object::new().property(
                "test",
                RefOr::T(Schema::OneOf(
                    OneOf::new()
                        .item(Schema::Array(Array::new(RefOr::T(Schema::Object(
                            Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
                        )))))
                        .item(Schema::Array(Array::new(RefOr::T(Schema::Object(
                            Object::new().property("foobar", RefOr::Ref(Ref::new("#/foobar"))),
                        ))))),
                )),
            ),
        ));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");

        let json_de_str = serde_json::to_string(&deserialized).expect("");
        println!("----------------------------");
        println!("{json_de_str}");

        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_all_of_of_within_ref_or_t_object_builder() {
        let ref_or_schema = RefOr::T(Schema::Object(
            Object::new().property(
                "test",
                RefOr::T(Schema::AllOf(
                    AllOf::new()
                        .item(Schema::Array(Array::new(RefOr::T(Schema::Object(
                            Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
                        )))))
                        .item(RefOr::T(Schema::Object(
                            Object::new().property("foobar", RefOr::Ref(Ref::new("#/foobar"))),
                        ))),
                )),
            ),
        ));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");

        let json_de_str = serde_json::to_string(&deserialized).expect("");
        println!("----------------------------");
        println!("{json_de_str}");

        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_any_of_of_within_ref_or_t_object_builder() {
        let ref_or_schema = RefOr::T(Schema::Object(
            Object::new().property(
                "test",
                RefOr::T(Schema::AnyOf(
                    AnyOf::new()
                        .item(Schema::Array(Array::new(RefOr::T(Schema::Object(
                            Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
                        )))))
                        .item(RefOr::T(Schema::Object(
                            Object::new().property("foobar", RefOr::Ref(Ref::new("#/foobar"))),
                        ))),
                )),
            ),
        ));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");

        let json_de_str = serde_json::to_string(&deserialized).expect("");
        println!("----------------------------");
        println!("{json_de_str}");
        assert!(json_str.contains("\"anyOf\""));
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_array_ref_or_t() {
        let ref_or_schema = RefOr::T(Schema::Array(Array::new(RefOr::T(Schema::Object(
            Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
        )))));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");

        let json_de_str = serde_json::to_string(&deserialized).expect("");
        println!("----------------------------");
        println!("{json_de_str}");

        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_array_builder() {
        let ref_or_schema = Array::new(RefOr::T(Schema::Object(
            Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
        )));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");

        let json_de_str = serde_json::to_string(&deserialized).expect("");
        println!("----------------------------");
        println!("{json_de_str}");

        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_with_additional_properties() {
        let schema = Schema::Object(Object::new().property(
            "map",
            Object::new().additional_properties(AdditionalProperties::FreeForm(true)),
        ));

        let json_str = serde_json::to_string(&schema).unwrap();
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).unwrap();

        let json_de_str = serde_json::to_string(&deserialized).unwrap();
        println!("----------------------------");
        println!("{json_de_str}");

        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_with_additional_properties_object() {
        let schema = Schema::Object(Object::new().property(
            "map",
            Object::new().additional_properties(Object::new().property("name", Object::with_type(SchemaType::String))),
        ));

        let json_str = serde_json::to_string(&schema).unwrap();
        println!("----------------------------");
        println!("{json_str}");

        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).unwrap();

        let json_de_str = serde_json::to_string(&deserialized).unwrap();
        println!("----------------------------");
        println!("{json_de_str}");

        assert_eq!(json_str, json_de_str);
    }
}
