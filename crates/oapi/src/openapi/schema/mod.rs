//! Implements [OpenAPI Schema Object][schema] types which can be
//! used to define field properties, enum values, array or object types.
//!
//! [schema]: https://spec.openapis.org/oas/latest.html#schema-object
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

use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};

use crate::{PropMap, RefOr};

/// Schemas collection for OpenApi.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Schemas(pub PropMap<String, RefOr<Schema>>);

impl<K, R> From<PropMap<K, R>> for Schemas
where
    K: Into<String>,
    R: Into<RefOr<Schema>>,
{
    fn from(inner: PropMap<K, R>) -> Self {
        Self(
            inner
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}
impl<K, R, const N: usize> From<[(K, R); N]> for Schemas
where
    K: Into<String>,
    R: Into<RefOr<Schema>>,
{
    fn from(inner: [(K, R); N]) -> Self {
        Self(
            <[(K, R)]>::into_vec(Box::new(inner))
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}

impl Deref for Schemas {
    type Target = PropMap<String, RefOr<Schema>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Schemas {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for Schemas {
    type Item = (String, RefOr<Schema>);
    type IntoIter = <PropMap<String, RefOr<Schema>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Schemas {
    /// Construct a new empty [`Schemas`]. This is effectively same as calling [`Schemas::default`].
    pub fn new() -> Self {
        Default::default()
    }
    /// Inserts a key-value pair into the instance and returns `self`.
    pub fn schema<K: Into<String>, V: Into<RefOr<Schema>>>(mut self, key: K, value: V) -> Self {
        self.insert(key, value);
        self
    }
    /// Inserts a key-value pair into the instance.
    pub fn insert<K: Into<String>, V: Into<RefOr<Schema>>>(&mut self, key: K, value: V) {
        self.0.insert(key.into(), value.into());
    }
    /// Moves all elements from `other` into `self`, leaving `other` empty.
    ///
    /// If a key from `other` is already present in `self`, the respective
    /// value from `self` will be overwritten with the respective value from `other`.
    pub fn append(&mut self, other: &mut Schemas) {
        let items = std::mem::take(&mut other.0);
        for item in items {
            self.insert(item.0, item.1);
        }
    }
    /// Extends a collection with the contents of an iterator.
    pub fn extend<I, K, V>(&mut self, iter: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<RefOr<Schema>>,
    {
        for (k, v) in iter.into_iter() {
            self.insert(k, v);
        }
    }
}

/// Create an _`empty`_ [`Schema`] that serializes to _`null`_.
///
/// Can be used in places where an item can be serialized as `null`. This is used with unit type
/// enum variants and tuple unit types.
pub fn empty() -> Schema {
    Schema::Object(
        Object::new()
            .schema_type(SchemaType::AnyValue)
            .default_value(serde_json::Value::Null),
    )
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
        Schema::Object(Default::default())
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

    /// An object to hold mappings between payload values and schema names or references.
    /// This field can only be populated manually. There is no macro support and no
    /// validation.
    #[serde(skip_serializing_if = "PropMap::is_empty", default)]
    pub mapping: PropMap<String, String>,
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
            mapping: PropMap::new(),
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
        Self::RefOr(RefOr::Type(Schema::Object(value)))
    }
}

impl From<Array> for AdditionalProperties<Schema> {
    fn from(value: Array) -> Self {
        Self::RefOr(RefOr::Type(Schema::Array(value)))
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

    /// A description which by default should override that of the referenced component.
    /// Description supports markdown syntax. If referenced object type does not support
    /// description this field does not have effect.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub description: String,

    /// A short summary which by default should override that of the referenced component. If
    /// referenced component does not support summary field this does not have effect.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub summary: String,
}

impl Ref {
    /// Construct a new [`Ref`] with custom ref location. In most cases this is not necessary
    /// and [`Ref::from_schema_name`] could be used instead.
    pub fn new<I: Into<String>>(ref_location: I) -> Self {
        Self {
            ref_location: ref_location.into(),
            ..Default::default()
        }
    }

    /// Construct a new [`Ref`] from provided schema name. This will create a [`Ref`] that
    /// references the reusable schemas.
    pub fn from_schema_name<I: Into<String>>(schema_name: I) -> Self {
        Self::new(format!("#/components/schemas/{}", schema_name.into()))
    }

    /// Construct a new [`Ref`] from provided response name. This will create a [`Ref`] that
    /// references the reusable response.
    pub fn from_response_name<I: Into<String>>(response_name: I) -> Self {
        Self::new(format!("#/components/responses/{}", response_name.into()))
    }

    /// Add or change reference location of the actual component.
    pub fn ref_location(mut self, ref_location: String) -> Self {
        self.ref_location = ref_location;
        self
    }

    /// Add or change reference location of the actual component automatically formatting the $ref
    /// to `#/components/schemas/...` format.
    pub fn ref_location_from_schema_name<S: Into<String>>(mut self, schema_name: S) -> Self {
        self.ref_location = format!("#/components/schemas/{}", schema_name.into());
        self
    }

    // TODO: REMOVE THE unnecesary description Option wrapping.

    /// Add or change description which by default should override that of the referenced component.
    /// Description supports markdown syntax. If referenced object type does not support
    /// description this field does not have effect.
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = description.into();
        self
    }

    /// Add or change short summary which by default should override that of the referenced component. If
    /// referenced component does not support summary field this does not have effect.
    pub fn summary<S: Into<String>>(mut self, summary: S) -> Self {
        self.summary = summary.into();
        self
    }
}

impl From<Ref> for RefOr<Schema> {
    fn from(r: Ref) -> Self {
        Self::Ref(r)
    }
}

impl<T> From<T> for RefOr<T> {
    fn from(t: T) -> Self {
        Self::Type(t)
    }
}

impl Default for RefOr<Schema> {
    fn default() -> Self {
        Self::Type(Schema::Object(Object::new()))
    }
}

impl ToArray for RefOr<Schema> {}

/// Represents type of [`Schema`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum SchemaType {
    /// Single type known from OpenAPI spec 3.0
    Basic(BasicType),
    /// Multiple types rendered as [`slice`]
    Array(Vec<BasicType>),
    /// Type that is considred typeless. _`AnyValue`_ will omit the type definition from the schema
    /// making it to accept any type possible.
    AnyValue,
}

impl Default for SchemaType {
    fn default() -> Self {
        Self::Basic(BasicType::default())
    }
}

impl From<BasicType> for SchemaType {
    fn from(value: BasicType) -> Self {
        SchemaType::basic(value)
    }
}

impl FromIterator<BasicType> for SchemaType {
    fn from_iter<T: IntoIterator<Item = BasicType>>(iter: T) -> Self {
        Self::Array(iter.into_iter().collect())
    }
}
impl SchemaType {
    /// Instantiate new [`SchemaType`] of given [`BasicType`]
    ///
    /// Method accpets one argument `type` to create [`SchemaType`] for.
    ///
    /// # Examples
    ///
    /// _**Create string [`SchemaType`]**_
    /// ```rust
    /// # use salvo_oapi::schema::{SchemaType, BasicType};
    /// let ty = SchemaType::basic(BasicType::String);
    /// ```
    pub fn basic(r#type: BasicType) -> Self {
        Self::Basic(r#type)
    }

    //// Instantiate new [`SchemaType::AnyValue`].
    ///
    /// This is same as calling [`SchemaType::AnyValue`] but in a function form `() -> SchemaType`
    /// allowing it to be used as argument for _serde's_ _`default = "..."`_.
    pub fn any() -> Self {
        SchemaType::AnyValue
    }

    /// Check whether this [`SchemaType`] is any value _(typeless)_ returning true on any value
    /// schema type.
    pub fn is_any_value(&self) -> bool {
        matches!(self, Self::AnyValue)
    }
}

/// Represents data type fragment of [`Schema`].
///
/// [`BasicType`] is used to create a [`SchemaType`] that defines the type of the [`Schema`].
/// [`SchemaType`] can be created from a single [`BasicType`] or multiple [`BasicType`]s according to the
/// OpenAPI 3.1 spec. Since the OpenAPI 3.1 is fully compatible with JSON schema the definiton of
/// the _**type**_ property comes from [JSON Schema type](https://json-schema.org/understanding-json-schema/reference/type).
///
/// # Examples
/// _**Create nullable string [`SchemaType`]**_
/// ```rust
/// # use std::iter::FromIterator;
/// # use salvo_oapi::schema::{BasicType, SchemaType};
/// let _: SchemaType = [BasicType::String, BasicType::Null].into_iter().collect();
/// ```
/// _**Create string [`SchemaType`]**_
/// ```rust
/// # use salvo_oapi::schema::{BasicType, SchemaType};
/// let _ = SchemaType::basic(BasicType::String);
/// ```
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BasicType {
    /// Used with [`Object`] to describe schema that has _properties_ describing fields. have
    #[default]
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
    /// Null type. Used together with other type to indicate nullable values.
    Null,
}

/// Additional format for [`SchemaType`] to fine tune the data type used.
///
/// If the **format** is not supported by the UI it may default back to [`SchemaType`] alone.
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
///
/// Known format is defined in <https://spec.openapis.org/oas/latest.html#data-types> and
/// <https://datatracker.ietf.org/doc/html/draft-bhutton-json-schema-validation-00#section-7.3> as
/// well as by few known data types that are enabled by specific feature flag e.g. _`uuid`_.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
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
    #[serde(rename = "uint8")]
    UInt8,
    /// 16 bit unsigned integer.
    #[serde(rename = "uint16")]
    UInt16,
    /// 32 bit unsigned integer.
    #[serde(rename = "uint32")]
    UInt32,
    /// 64 bit unsigned integer.
    #[serde(rename = "uint64")]
    UInt64,
    /// floating point number.
    Float,
    /// double (floating point) number.
    Double,
    /// base64 encoded chars.
    Byte,
    /// binary data (octet).
    Binary,
    /// ISO-8601 full time format [RFC3339](https://xml2rfc.ietf.org/public/rfc/html/rfc3339.html#anchor14).
    Time,
    /// ISO-8601 full date [RFC3339](https://xml2rfc.ietf.org/public/rfc/html/rfc3339.html#anchor14).
    Date,
    /// ISO-8601 full date time [RFC3339](https://xml2rfc.ietf.org/public/rfc/html/rfc3339.html#anchor14).
    DateTime,
    /// duration format from [RFC3339 Appendix-A](https://datatracker.ietf.org/doc/html/rfc3339#appendix-A).
    Duration,
    /// Hint to UI to obscure input.
    Password,
    /// Use for compact string
    String,
    /// Used with [`String`] values to indicate value is in decimal format.
    ///
    /// **decimal** feature need to be enabled.
    #[cfg(any(feature = "decimal", feature = "decimal-float"))]
    #[cfg_attr(docsrs, doc(cfg(any(feature = "decimal", feature = "decimal-float"))))]
    Decimal,
    /// Used with [`String`] values to indicate value is in ULID format.
    #[cfg(feature = "ulid")]
    #[cfg_attr(docsrs, doc(cfg(feature = "ulid")))]
    Ulid,

    /// Used with [`String`] values to indicate value is in UUID format.
    #[cfg(feature = "uuid")]
    #[cfg_attr(docsrs, doc(cfg(feature = "uuid")))]
    Uuid,
    /// Used with [`String`] values to indicate value is in Url format.
    ///
    /// **url** feature need to be enabled.
    #[cfg(feature = "url")]
    #[cfg_attr(docsrs, doc(cfg(feature = "url")))]
    Url,
    /// A string instance is valid against this attribute if it is a valid URI Reference
    /// (either a URI or a relative-reference) according to
    /// [RFC3986](https://datatracker.ietf.org/doc/html/rfc3986).
    #[cfg(feature = "url")]
    #[cfg_attr(docsrs, doc(cfg(feature = "url")))]
    UriReference,
    /// A string instance is valid against this attribute if it is a
    /// valid IRI, according to [RFC3987](https://datatracker.ietf.org/doc/html/rfc3987).
    #[cfg(feature = "url")]
    #[cfg_attr(docsrs, doc(cfg(feature = "url")))]
    Iri,
    /// A string instance is valid against this attribute if it is a valid IRI Reference
    /// (either an IRI or a relative-reference)
    /// according to [RFC3987](https://datatracker.ietf.org/doc/html/rfc3987).
    #[cfg(feature = "url")]
    #[cfg_attr(docsrs, doc(cfg(feature = "url")))]
    IriReference,
    /// As defined in "Mailbox" rule [RFC5321](https://datatracker.ietf.org/doc/html/rfc5321#section-4.1.2).
    Email,
    /// As defined by extended "Mailbox" rule [RFC6531](https://datatracker.ietf.org/doc/html/rfc6531#section-3.3).
    IdnEmail,
    /// As defined by [RFC1123](https://datatracker.ietf.org/doc/html/rfc1123#section-2.1), including host names
    /// produced using the Punycode algorithm
    /// specified in [RFC5891](https://datatracker.ietf.org/doc/html/rfc5891#section-4.4).
    Hostname,
    /// As defined by either [RFC1123](https://datatracker.ietf.org/doc/html/rfc1123#section-2.1) as for hostname,
    /// or an internationalized hostname as defined by [RFC5890](https://datatracker.ietf.org/doc/html/rfc5890#section-2.3.2.3).
    IdnHostname,
    /// An IPv4 address according to [RFC2673](https://datatracker.ietf.org/doc/html/rfc2673#section-3.2).
    Ipv4,
    /// An IPv6 address according to [RFC4291](https://datatracker.ietf.org/doc/html/rfc4291#section-2.2).
    Ipv6,
    /// A string instance is a valid URI Template if it is according to
    /// [RFC6570](https://datatracker.ietf.org/doc/html/rfc6570).
    ///
    /// _**Note!**_ There are no separate IRL template.
    UriTemplate,
    /// A valid JSON string representation of a JSON Pointer according to [RFC6901](https://datatracker.ietf.org/doc/html/rfc6901#section-5).
    JsonPointer,
    /// A valid relative JSON Pointer according to [draft-handrews-relative-json-pointer-01](https://datatracker.ietf.org/doc/html/draft-handrews-relative-json-pointer-01).
    RelativeJsonPointer,
    /// Regular expression, which SHOULD be valid according to the
    /// [ECMA-262](https://datatracker.ietf.org/doc/html/draft-bhutton-json-schema-validation-00#ref-ecma262).
    Regex,
}

#[cfg(test)]
mod tests {
    use assert_json_diff::assert_json_eq;
    use serde_json::{Value, json};

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
                                    .schema_type(BasicType::Integer)
                                    .format(SchemaFormat::KnownFormat(KnownFormat::Int32))
                                    .description("Id of credential")
                                    .default_value(json!(1i32)),
                            )
                            .property(
                                "name",
                                Object::new()
                                    .schema_type(BasicType::String)
                                    .description("Name of credential"),
                            )
                            .property(
                                "status",
                                Object::new()
                                    .schema_type(BasicType::String)
                                    .default_value(json!("Active"))
                                    .description("Credential status")
                                    .enum_values(["Active", "NotActive", "Locked", "Expired"]),
                            )
                            .property(
                                "history",
                                Array::new().items(Ref::from_schema_name("UpdateHistory")),
                            )
                            .property("tags", Object::with_type(BasicType::String).to_array()),
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
            credential
                .get("id")
                .unwrap_or(&serde_json::value::Value::Null),
            json!({"type":"integer","format":"int32","description":"Id of credential","default":1})
        );
        assert_json_eq!(
            credential
                .get("name")
                .unwrap_or(&serde_json::value::Value::Null),
            json!({"type":"string","description":"Name of credential"})
        );
        assert_json_eq!(
            credential
                .get("status")
                .unwrap_or(&serde_json::value::Value::Null),
            json!({"default":"Active","description":"Credential status","enum":["Active","NotActive","Locked","Expired"],"type":"string"})
        );
        assert_json_eq!(
            credential
                .get("history")
                .unwrap_or(&serde_json::value::Value::Null),
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
                    .schema_type(BasicType::Integer)
                    .format(SchemaFormat::KnownFormat(KnownFormat::Int32))
                    .description("Id of credential")
                    .default_value(json!(1i32)),
            )
            .property(
                "name",
                Object::new()
                    .schema_type(BasicType::String)
                    .description("Name of credential"),
            )
            .property(
                "status",
                Object::new()
                    .schema_type(BasicType::String)
                    .default_value(json!("Active"))
                    .description("Credential status")
                    .enum_values(["Active", "NotActive", "Locked", "Expired"]),
            )
            .property(
                "history",
                Array::new().items(Ref::from_schema_name("UpdateHistory")),
            )
            .property("tags", Object::with_type(BasicType::String).to_array());

        #[cfg(not(feature = "preserve-order"))]
        assert_eq!(
            json_value.properties.keys().collect::<Vec<_>>(),
            vec!["history", "id", "name", "status", "tags"]
        );

        #[cfg(feature = "preserve-order")]
        assert_eq!(
            json_value.properties.keys().collect::<Vec<_>>(),
            vec!["id", "name", "status", "history", "tags"]
        );
    }

    // Examples taken from https://spec.openapis.org/oas/latest.html#model-with-map-dictionary-properties
    #[test]
    fn test_additional_properties() {
        let json_value =
            Object::new().additional_properties(Object::new().schema_type(BasicType::String));
        assert_json_eq!(
            json_value,
            json!({
                "type": "object",
                "additionalProperties": {
                    "type": "string"
                }
            })
        );

        let json_value = Object::new().additional_properties(
            Array::new().items(Object::new().schema_type(BasicType::Number)),
        );
        assert_json_eq!(
            json_value,
            json!({
                "type": "object",
                "additionalProperties": {
                    "items": {
                        "type": "number",
                    },
                    "type": "array",
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
    fn test_object_with_name() {
        let json_value = Object::new().name("SomeName");
        assert_json_eq!(
            json_value,
            json!({
                "type": "object",
                "name": "SomeName"
            })
        );
    }

    #[test]
    fn test_derive_object_with_examples() {
        let expected = r#"{"type":"object","examples":[{"age":20,"name":"bob the cat"}]}"#;
        let json_value = Object::new().examples([json!({"age": 20, "name": "bob the cat"})]);

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
        let array = Array::new().items(
            Object::new().property(
                "id",
                Object::new()
                    .schema_type(BasicType::Integer)
                    .format(SchemaFormat::KnownFormat(KnownFormat::Int32))
                    .description("Id of credential")
                    .default_value(json!(1i32)),
            ),
        );

        assert!(matches!(
            array.schema_type,
            SchemaType::Basic(BasicType::Array)
        ));
    }

    #[test]
    fn test_array_builder() {
        let array: Array = Array::new().items(
            Object::new().property(
                "id",
                Object::new()
                    .schema_type(BasicType::Integer)
                    .format(SchemaFormat::KnownFormat(KnownFormat::Int32))
                    .description("Id of credential")
                    .default_value(json!(1i32)),
            ),
        );

        assert!(matches!(
            array.schema_type,
            SchemaType::Basic(BasicType::Array)
        ));
    }

    #[test]
    fn reserialize_deserialized_schema_components() {
        let components = Components::new()
            .extend_schemas(vec![(
                "Comp",
                Schema::from(
                    Object::new()
                        .property("name", Object::new().schema_type(BasicType::String))
                        .required("name"),
                ),
            )])
            .response("204", Response::new("No Content"))
            .extend_responses(vec![("200", Response::new("Okay"))])
            .add_security_scheme("TLS", SecurityScheme::MutualTls { description: None })
            .extend_security_schemes(vec![(
                "APIKey",
                SecurityScheme::Http(security::Http::default()),
            )]);

        let serialized_components = serde_json::to_string(&components).unwrap();

        let deserialized_components: Components =
            serde_json::from_str(serialized_components.as_str()).unwrap();

        assert_eq!(
            serialized_components,
            serde_json::to_string(&deserialized_components).unwrap()
        )
    }

    #[test]
    fn reserialize_deserialized_object_component() {
        let prop = Object::new()
            .property("name", Object::new().schema_type(BasicType::String))
            .required("name");

        let serialized_components = serde_json::to_string(&prop).unwrap();
        let deserialized_components: Object =
            serde_json::from_str(serialized_components.as_str()).unwrap();

        assert_eq!(
            serialized_components,
            serde_json::to_string(&deserialized_components).unwrap()
        )
    }

    #[test]
    fn reserialize_deserialized_property() {
        let prop = Object::new().schema_type(BasicType::String);

        let serialized_components = serde_json::to_string(&prop).unwrap();
        let deserialized_components: Object =
            serde_json::from_str(serialized_components.as_str()).unwrap();

        assert_eq!(
            serialized_components,
            serde_json::to_string(&deserialized_components).unwrap()
        )
    }

    #[test]
    fn serialize_deserialize_array_within_ref_or_t_object_builder() {
        let ref_or_schema = RefOr::Type(Schema::Object(Object::new().property(
            "test",
            RefOr::Type(Schema::Array(Array::new().items(RefOr::Type(
                Schema::Object(Object::new().property("element", RefOr::Ref(Ref::new("#/test")))),
            )))),
        )));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");
        let json_de_str = serde_json::to_string(&deserialized).expect("");
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_one_of_within_ref_or_t_object_builder() {
        let ref_or_schema = RefOr::Type(Schema::Object(
            Object::new().property(
                "test",
                RefOr::Type(Schema::OneOf(
                    OneOf::new()
                        .item(Schema::Array(Array::new().items(RefOr::Type(
                            Schema::Object(
                                Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
                            ),
                        ))))
                        .item(Schema::Array(Array::new().items(RefOr::Type(
                            Schema::Object(
                                Object::new().property("foobar", RefOr::Ref(Ref::new("#/foobar"))),
                            ),
                        )))),
                )),
            ),
        ));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");
        let json_de_str = serde_json::to_string(&deserialized).expect("");
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_all_of_of_within_ref_or_t_object() {
        let ref_or_schema = RefOr::Type(Schema::Object(
            Object::new().property(
                "test",
                RefOr::Type(Schema::AllOf(
                    AllOf::new()
                        .item(Schema::Array(Array::new().items(RefOr::Type(
                            Schema::Object(
                                Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
                            ),
                        ))))
                        .item(RefOr::Type(Schema::Object(
                            Object::new().property("foobar", RefOr::Ref(Ref::new("#/foobar"))),
                        ))),
                )),
            ),
        ));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");
        let json_de_str = serde_json::to_string(&deserialized).expect("");
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_any_of_of_within_ref_or_t_object() {
        let ref_or_schema = RefOr::Type(Schema::Object(
            Object::new().property(
                "test",
                RefOr::Type(Schema::AnyOf(
                    AnyOf::new()
                        .item(Schema::Array(Array::new().items(RefOr::Type(
                            Schema::Object(
                                Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
                            ),
                        ))))
                        .item(RefOr::Type(Schema::Object(
                            Object::new().property("foobar", RefOr::Ref(Ref::new("#/foobar"))),
                        ))),
                )),
            ),
        ));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");
        let json_de_str = serde_json::to_string(&deserialized).expect("");
        assert!(json_str.contains("\"anyOf\""));
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_array_ref_or_t() {
        let ref_or_schema = RefOr::Type(Schema::Array(Array::new().items(RefOr::Type(
            Schema::Object(Object::new().property("element", RefOr::Ref(Ref::new("#/test")))),
        ))));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");
        let json_de_str = serde_json::to_string(&deserialized).expect("");
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_array() {
        let ref_or_schema = Array::new().items(RefOr::Type(Schema::Object(
            Object::new().property("element", RefOr::Ref(Ref::new("#/test"))),
        )));

        let json_str = serde_json::to_string(&ref_or_schema).expect("");
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).expect("");
        let json_de_str = serde_json::to_string(&deserialized).expect("");
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_with_additional_properties() {
        let schema = Schema::Object(Object::new().property(
            "map",
            Object::new().additional_properties(AdditionalProperties::FreeForm(true)),
        ));

        let json_str = serde_json::to_string(&schema).unwrap();
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).unwrap();
        let json_de_str = serde_json::to_string(&deserialized).unwrap();
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_deserialize_schema_with_additional_properties_object() {
        let schema = Schema::Object(Object::new().property(
            "map",
            Object::new().additional_properties(
                Object::new().property("name", Object::with_type(BasicType::String)),
            ),
        ));

        let json_str = serde_json::to_string(&schema).unwrap();
        let deserialized: RefOr<Schema> = serde_json::from_str(&json_str).unwrap();
        let json_de_str = serde_json::to_string(&deserialized).unwrap();
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn serialize_discriminator_with_mapping() {
        let mut discriminator = Discriminator::new("type");
        discriminator.mapping = [("int".to_string(), "#/components/schemas/MyInt".to_string())]
            .into_iter()
            .collect::<PropMap<_, _>>();
        let one_of = OneOf::new()
            .item(Ref::from_schema_name("MyInt"))
            .discriminator(discriminator);
        let json_value = serde_json::to_value(one_of).unwrap();

        assert_json_eq!(
            json_value,
            json!({
                "oneOf": [
                    {
                        "$ref": "#/components/schemas/MyInt"
                    }
                ],
                "discriminator": {
                    "propertyName": "type",
                    "mapping": {
                        "int": "#/components/schemas/MyInt"
                    }
                }
            })
        );
    }

    #[test]
    fn deserialize_reserialize_one_of_default_type() {
        let a = OneOf::new()
            .item(Schema::Array(Array::new().items(RefOr::Type(
                Schema::Object(Object::new().property("element", RefOr::Ref(Ref::new("#/test")))),
            ))))
            .item(Schema::Array(Array::new().items(RefOr::Type(
                Schema::Object(Object::new().property("foobar", RefOr::Ref(Ref::new("#/foobar")))),
            ))));

        let serialized_json = serde_json::to_string(&a).expect("should serialize to json");
        let b: OneOf = serde_json::from_str(&serialized_json).expect("should deserialize OneOf");
        let reserialized_json = serde_json::to_string(&b).expect("reserialized json");

        assert_eq!(serialized_json, reserialized_json);
    }

    #[test]
    fn serialize_deserialize_object_with_multiple_schema_types() {
        let object =
            Object::new().schema_type(SchemaType::from_iter([BasicType::Object, BasicType::Null]));

        let json_str = serde_json::to_string(&object).unwrap();
        let deserialized: Object = serde_json::from_str(&json_str).unwrap();
        let json_de_str = serde_json::to_string(&deserialized).unwrap();
        assert_eq!(json_str, json_de_str);
    }

    #[test]
    fn test_empty_schema() {
        let schema = empty();
        assert_json_eq!(
            schema,
            json!({
                "default": null
            })
        )
    }

    #[test]
    fn test_default_schema() {
        let schema = Schema::default();
        assert_json_eq!(
            schema,
            json!({
                "type": "object",
            })
        )
    }

    #[test]
    fn test_ref_from_response_name() {
        let _ref = Ref::from_response_name("MyResponse");
        assert_json_eq!(
            _ref,
            json!({
                "$ref": "#/components/responses/MyResponse"
            })
        )
    }

    #[test]
    fn test_additional_properties_from_ref_or() {
        let additional_properties =
            AdditionalProperties::from(RefOr::Type(Schema::Object(Object::new())));
        assert_json_eq!(
            additional_properties,
            json!({
                "type": "object",
            })
        )
    }
}
