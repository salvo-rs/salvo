//! Implements [OpenAPI Xml Object][xml_object] types.
//!
//! [xml_object]: https://spec.openapis.org/oas/latest.html#xml-object
use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use super::set_value;

/// Implements [OpenAPI Xml Object][xml_object].
///
/// Can be used to modify xml output format of specific [OpenAPI Schema Object][schema_object] which are
/// implemented in [`schema`][schema] module.
///
/// [xml_object]: https://spec.openapis.org/oas/latest.html#xml-object
/// [schema_object]: https://spec.openapis.org/oas/latest.html#schema-object
/// [schema]: ../schema/index.html
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct Xml {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribute: Option<bool>,

    /// Flag only usable with array definition. If set to true the output xml will wrap the array of items
    /// `<pets><pet></pet></pets>` instead of unwrapped `<pet></pet>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapped: Option<bool>,
}

impl Xml {
    /// Construct a new [`Xml`] object.
    pub fn new() -> Self {
        Self { ..Default::default() }
    }
}

impl Xml {
    /// Add [`Xml::name`] to xml object.
    ///
    /// Builder style chainable consuming add name method.
    pub fn name<S: Into<Cow<'static, str>>>(mut self, name: S) -> Self {
        set_value!(self name Some(name.into()))
    }

    /// Add [`Xml::namespace`] to xml object.
    ///
    /// Builder style chainable consuming add namespace method.
    pub fn namespace<S: Into<Cow<'static, str>>>(mut self, namespace: S) -> Self {
        set_value!(self namespace Some(namespace.into()))
    }

    /// Add [`Xml::prefix`] to xml object.
    ///
    /// Builder style chainable consuming add prefix method.
    pub fn prefix<S: Into<Cow<'static, str>>>(mut self, prefix: S) -> Self {
        set_value!(self prefix Some(prefix.into()))
    }

    /// Mark [`Xml`] object as attribute. See [`Xml::attribute`]
    ///
    /// Builder style chainable consuming add attribute method.
    pub fn attribute(mut self, attribute: bool) -> Self {
        set_value!(self attribute Some(attribute))
    }

    /// Mark [`Xml`] object wrapped. See [`Xml::wrapped`]
    ///
    /// Builder style chainable consuming add wrapped method.
    pub fn wrapped(mut self, wrapped: bool) -> Self {
        set_value!(self wrapped Some(wrapped))
    }
}

#[cfg(test)]
mod tests {
    use super::Xml;

    #[test]
    fn xml_new() {
        let xml = Xml::new();

        assert!(xml.name.is_none());
        assert!(xml.namespace.is_none());
        assert!(xml.prefix.is_none());
        assert!(xml.attribute.is_none());
        assert!(xml.wrapped.is_none());
    }
}
