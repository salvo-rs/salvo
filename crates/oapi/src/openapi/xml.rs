//! Implements [OpenAPI Xml Object][xml_object] types.
//!
//! [xml_object]: https://spec.openapis.org/oas/latest.html#xml-object
use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// Implements [OpenAPI Xml Object][xml_object].
///
/// Can be used to modify xml output format of specific [OpenAPI Schema Object][schema_object] which
/// are implemented in [`schema`][schema] module.
///
/// [xml_object]: https://spec.openapis.org/oas/latest.html#xml-object
/// [schema_object]: https://spec.openapis.org/oas/latest.html#schema-object
/// [schema]: ../schema/index.html
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Xml {
    /// The kind of XML node the schema corresponds to. Added in OpenAPI 3.2.
    ///
    /// When set, [`Xml::attribute`] and [`Xml::wrapped`] must not be used — `nodeType` is the
    /// replacement for both.
    ///
    /// See <https://spec.openapis.org/oas/v3.2.0.html#xml-object>.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<XmlNodeType>,

    /// Used to replace the name of attribute or type used in schema property.
    /// When used with [`Xml::wrapped`] attribute the name will be used as a wrapper name
    /// for wrapped array instead of the item or type name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<Cow<'static, str>>,

    /// Valid uri definition of namespace used in xml.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<Cow<'static, str>>,

    /// Prefix for xml element [`Xml::name`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<Cow<'static, str>>,

    /// Flag deciding will this attribute translate to element attribute instead of xml element.
    ///
    /// Deprecated in OpenAPI 3.2 in favour of [`XmlNodeType::Attribute`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribute: Option<bool>,

    /// Flag only usable with array definition. If set to true the output xml will wrap the array
    /// of items `<pets><pet></pet></pets>` instead of unwrapped `<pet></pet>`.
    ///
    /// Deprecated in OpenAPI 3.2 in favour of [`XmlNodeType::Element`] on the array schema.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapped: Option<bool>,
}

/// The kind of XML [DOM node](https://dom.spec.whatwg.org/#interface-node) a schema describes.
///
/// Used by the OpenAPI 3.2 [`Xml::node_type`] field.
///
/// See <https://spec.openapis.org/oas/v3.2.0.html#xml-node-types>.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum XmlNodeType {
    /// The schema represents an element and describes its contents.
    Element,
    /// The schema represents an attribute and describes its value.
    Attribute,
    /// The schema represents a text node (parsed character data).
    Text,
    /// The schema represents a CDATA section.
    Cdata,
    /// The schema does not correspond to any node; nodes for its subschemas are placed
    /// directly under the parent schema's node.
    None,
}

impl Xml {
    /// Construct a new [`Xml`] object.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }
}

impl Xml {
    /// Set [`Xml::node_type`]. Requires OpenAPI 3.2.
    ///
    /// Builder style chainable consuming add node type method.
    #[must_use]
    pub fn node_type(mut self, node_type: XmlNodeType) -> Self {
        self.node_type = Some(node_type);
        self
    }

    /// Add [`Xml::name`] to xml object.
    ///
    /// Builder style chainable consuming add name method.
    #[must_use]
    pub fn name<S: Into<Cow<'static, str>>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Add [`Xml::namespace`] to xml object.
    ///
    /// Builder style chainable consuming add namespace method.
    #[must_use]
    pub fn namespace<S: Into<Cow<'static, str>>>(mut self, namespace: S) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Add [`Xml::prefix`] to xml object.
    ///
    /// Builder style chainable consuming add prefix method.
    #[must_use]
    pub fn prefix<S: Into<Cow<'static, str>>>(mut self, prefix: S) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Mark [`Xml`] object as attribute. See [`Xml::attribute`]
    ///
    /// Builder style chainable consuming add attribute method.
    #[must_use]
    pub fn attribute(mut self, attribute: bool) -> Self {
        self.attribute = Some(attribute);
        self
    }

    /// Mark [`Xml`] object wrapped. See [`Xml::wrapped`]
    ///
    /// Builder style chainable consuming add wrapped method.
    #[must_use]
    pub fn wrapped(mut self, wrapped: bool) -> Self {
        self.wrapped = Some(wrapped);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{Xml, XmlNodeType};

    #[test]
    fn xml_new() {
        let mut xml = Xml::new();

        assert!(xml.name.is_none());
        assert!(xml.namespace.is_none());
        assert!(xml.prefix.is_none());
        assert!(xml.attribute.is_none());
        assert!(xml.wrapped.is_none());

        xml = xml.name("name");
        assert!(xml.name.is_some());

        xml = xml.namespace("namespace");
        assert!(xml.namespace.is_some());

        xml = xml.prefix("prefix");
        assert!(xml.prefix.is_some());

        xml = xml.attribute(true);
        assert!(xml.attribute.is_some());

        xml = xml.wrapped(true);
        assert!(xml.wrapped.is_some());
    }

    #[test]
    fn xml_node_type_round_trips() {
        for (node_type, rendered) in [
            (XmlNodeType::Element, "element"),
            (XmlNodeType::Attribute, "attribute"),
            (XmlNodeType::Text, "text"),
            (XmlNodeType::Cdata, "cdata"),
            (XmlNodeType::None, "none"),
        ] {
            let xml = Xml::new().node_type(node_type);
            let value = serde_json::to_value(&xml).expect("serialize");
            assert_eq!(value, serde_json::json!({ "nodeType": rendered }));
            let parsed: Xml = serde_json::from_value(value).expect("deserialize");
            assert_eq!(parsed.node_type, Some(node_type));
        }
    }

    #[test]
    fn xml_without_node_type_is_unchanged() {
        let xml = Xml::new().name("pet").wrapped(true);
        assert_eq!(
            serde_json::to_value(&xml).expect("serialize"),
            serde_json::json!({ "name": "pet", "wrapped": true })
        );
    }
}
