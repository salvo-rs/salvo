//! Provides serde related features parsing serde attributes from types.
//!
//! Read more: <https://salvo.rs>
#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use proc_macro2::{Ident, Span, TokenTree};
use syn::buffer::Cursor;
use syn::{Attribute, Error};

pub(crate) mod case;
pub use case::RenameRule;

#[inline]
fn parse_next_lit_str(next: Cursor) -> Option<(String, Span)> {
    match next.token_tree() {
        Some((tt, next)) => match tt {
            TokenTree::Punct(punct) if punct.as_char() == '=' => parse_next_lit_str(next),
            TokenTree::Literal(literal) => {
                Some((literal.to_string().replace('\"', ""), literal.span()))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Value type of a `#[serde(...)]` attribute.
#[derive(Default, Debug)]
pub struct SerdeValue {
    /// Skip field.
    pub skip: bool,
    /// Rename field.
    pub rename: Option<String>,
    /// Aliases of field.
    pub aliases: Vec<String>,
    /// Is default value.
    pub is_default: bool,
    /// Flatten field.
    pub flatten: bool,
    /// Skip serializing if.
    pub skip_serializing_if: bool,
    /// Double option.
    pub double_option: bool,
}

impl SerdeValue {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut value = Self::default();

        input.step(|cursor| {
            let mut rest = *cursor;
            while let Some((tt, next)) = rest.token_tree() {
                match tt {
                    TokenTree::Ident(ident)
                        if ident == "skip"
                            || ident == "skip_serializing"
                            || ident == "skip_deserializing" =>
                    {
                        value.skip = true
                    }
                    TokenTree::Ident(ident) if ident == "skip_serializing_if" => {
                        value.skip_serializing_if = true
                    }
                    TokenTree::Ident(ident) if ident == "flatten" => value.flatten = true,
                    TokenTree::Ident(ident) if ident == "rename" => {
                        if let Some((literal, _)) = parse_next_lit_str(next) {
                            value.rename = Some(literal)
                        };
                    }
                    TokenTree::Ident(ident) if ident == "alias" => {
                        if let Some((literal, _)) = parse_next_lit_str(next) {
                            value.aliases.push(literal)
                        };
                    }
                    TokenTree::Ident(ident) if ident == "default" => value.is_default = true,
                    _ => (),
                }

                rest = next;
            }
            Ok(((), rest))
        })?;

        Ok(value)
    }
}

/// The [Serde Enum representation](https://serde.rs/enum-representations.html) being used
/// The default case (when no serde attributes are present) is `ExternallyTagged`.
#[derive(Default, Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub enum SerdeEnumRepr {
    /// ExternallyTagged.
    #[default]
    ExternallyTagged,
    /// InternallyTagged.
    InternallyTagged {
        /// tag.
        tag: String,
    },
    /// AdjacentlyTagged
    AdjacentlyTagged {
        /// tag.
        tag: String,
        /// content.
        content: String,
    },
    /// Untagged
    Untagged,
    /// This is a variant that can never happen because `serde` will not accept it.
    /// With the current implementation it is necessary to have it as an intermediate state when
    /// parsing the attributes
    UnfinishedAdjacentlyTagged {
        /// content.
        content: String,
    },
}

/// Attributes defined within a `#[serde(...)]` container attribute.
#[derive(Default, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct SerdeContainer {
    /// Rename all fields.
    pub rename_all: Option<RenameRule>,
    /// Enum repr.
    pub enum_repr: SerdeEnumRepr,
    /// Is default.
    pub is_default: bool,
    /// Deny unknown fields.
    pub deny_unknown_fields: bool,
}

impl SerdeContainer {
    /// Parse a single serde attribute, currently supported attributes are:
    ///     * `rename_all = ...`
    ///     * `tag = ...`
    ///     * `content = ...`
    ///     * `untagged = ...`
    ///     * `default = ...`
    fn parse_attribute(&mut self, ident: &Ident, next: Cursor) -> syn::Result<()> {
        match ident.to_string().as_str() {
            "rename_all" => {
                if let Some((literal, span)) = parse_next_lit_str(next) {
                    self.rename_all = Some(
                        literal
                            .parse::<RenameRule>()
                            .map_err(|error| Error::new(span, error.to_string()))?,
                    );
                }
            }
            "tag" => {
                if let Some((literal, span)) = parse_next_lit_str(next) {
                    self.enum_repr = match &self.enum_repr {
                        SerdeEnumRepr::ExternallyTagged => {
                            SerdeEnumRepr::InternallyTagged { tag: literal }
                        }
                        SerdeEnumRepr::UnfinishedAdjacentlyTagged { content } => {
                            SerdeEnumRepr::AdjacentlyTagged {
                                tag: literal,
                                content: content.clone(),
                            }
                        }
                        SerdeEnumRepr::InternallyTagged { .. }
                        | SerdeEnumRepr::AdjacentlyTagged { .. } => {
                            return Err(Error::new(span, "Duplicate serde tag argument"));
                        }
                        SerdeEnumRepr::Untagged => {
                            return Err(Error::new(span, "Untagged enum cannot have tag"));
                        }
                    };
                }
            }
            "content" => {
                if let Some((literal, span)) = parse_next_lit_str(next) {
                    self.enum_repr = match &self.enum_repr {
                        SerdeEnumRepr::InternallyTagged { tag } => {
                            SerdeEnumRepr::AdjacentlyTagged {
                                tag: tag.clone(),
                                content: literal,
                            }
                        }
                        SerdeEnumRepr::ExternallyTagged => {
                            SerdeEnumRepr::UnfinishedAdjacentlyTagged { content: literal }
                        }
                        SerdeEnumRepr::AdjacentlyTagged { .. }
                        | SerdeEnumRepr::UnfinishedAdjacentlyTagged { .. } => {
                            return Err(Error::new(span, "Duplicate serde content argument"));
                        }
                        SerdeEnumRepr::Untagged => {
                            return Err(Error::new(span, "Untagged enum cannot have content"));
                        }
                    };
                }
            }
            "untagged" => {
                self.enum_repr = SerdeEnumRepr::Untagged;
            }
            "default" => {
                self.is_default = true;
            }
            "deny_unknown_fields" => {
                self.deny_unknown_fields = true;
            }
            _ => {}
        }
        Ok(())
    }

    /// Parse the attributes inside a `#[serde(...)]` container attribute.
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut container = Self::default();

        input.step(|cursor| {
            let mut rest = *cursor;
            while let Some((tt, next)) = rest.token_tree() {
                if let TokenTree::Ident(ident) = tt {
                    container.parse_attribute(&ident, next)?
                }

                rest = next;
            }
            Ok(((), rest))
        })?;

        Ok(container)
    }
}

/// Parse value.
#[must_use]
pub fn parse_value(attributes: &[Attribute]) -> Option<SerdeValue> {
    attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("serde"))
        .map(|serde_attribute| serde_attribute.parse_args_with(SerdeValue::parse))
        .try_fold(SerdeValue::default(), |mut acc, value| {
            let Ok(value) = value else {
                return Some(acc);
            };
            if value.skip {
                acc.skip = value.skip;
            }
            if value.skip_serializing_if {
                acc.skip_serializing_if = value.skip_serializing_if;
            }
            if value.rename.is_some() {
                acc.rename = value.rename;
            }
            acc.aliases.extend(value.aliases);
            if value.flatten {
                acc.flatten = value.flatten;
            }
            if value.is_default {
                acc.is_default = value.is_default;
            }
            if value.double_option {
                acc.double_option = value.double_option;
            }

            Some(acc)
        })
}

/// Parse container.
#[must_use]
pub fn parse_container(attributes: &[Attribute]) -> Option<SerdeContainer> {
    attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("serde"))
        .map(|serde_attribute| serde_attribute.parse_args_with(SerdeContainer::parse))
        .try_fold(SerdeContainer::default(), |mut acc, value| {
            let Ok(value) = value else {
                return Some(acc);
            };
            if value.is_default {
                acc.is_default = value.is_default;
            }
            if value.deny_unknown_fields {
                acc.deny_unknown_fields = value.deny_unknown_fields;
            }
            match value.enum_repr {
                SerdeEnumRepr::ExternallyTagged => {}
                SerdeEnumRepr::Untagged
                | SerdeEnumRepr::InternallyTagged { .. }
                | SerdeEnumRepr::AdjacentlyTagged { .. }
                | SerdeEnumRepr::UnfinishedAdjacentlyTagged { .. } => {
                    acc.enum_repr = value.enum_repr;
                }
            }
            if value.rename_all.is_some() {
                acc.rename_all = value.rename_all;
            }

            Some(acc)
        })
}

#[cfg(test)]
mod tests {
    use syn::{Attribute, parse_quote};

    use super::case::RENAME_RULES;
    use super::{RenameRule, SerdeContainer, parse_container, parse_value};

    #[test]
    fn test_serde_parse_value() {
        let skip_attribute: syn::Attribute = parse_quote! {
            #[serde(skip)]
        };
        let rename_attribute: syn::Attribute = parse_quote! {
            #[serde(rename = "new_name")]
        };
        let default_attribute: syn::Attribute = parse_quote! {
            #[serde(default)]
        };
        let flatten_attribute: syn::Attribute = parse_quote! {
            #[serde(flatten)]
        };
        let skip_serializing_if_attribute: syn::Attribute = parse_quote! {
            #[serde(skip_serializing_if = "Option::is_none")]
        };
        let attributes: &[Attribute] = &[
            skip_attribute,
            rename_attribute,
            default_attribute,
            flatten_attribute,
            skip_serializing_if_attribute,
        ];

        let result = parse_value(attributes).unwrap();
        assert!(result.skip);
        assert_eq!(result.rename.unwrap(), "new_name");
        assert!(result.is_default);
        assert!(result.flatten);
        assert!(result.skip_serializing_if);
    }

    #[test]
    fn test_serde_parse_container() {
        let default_attribute_1: syn::Attribute = parse_quote! {
            #[serde(default)]
        };
        let default_attribute_2: syn::Attribute = parse_quote! {
            #[serde(default)]
        };
        let deny_unknown_fields_attribute: syn::Attribute = parse_quote! {
            #[serde(deny_unknown_fields)]
        };
        let unsupported_attribute: syn::Attribute = parse_quote! {
            #[serde(expecting = "...")]
        };
        let attributes: &[Attribute] = &[
            default_attribute_1,
            default_attribute_2,
            deny_unknown_fields_attribute,
            unsupported_attribute,
        ];

        let expected = SerdeContainer {
            is_default: true,
            deny_unknown_fields: true,
            ..Default::default()
        };

        let result = parse_container(attributes).unwrap();
        assert_eq!(expected.is_default, result.is_default);
        assert_eq!(expected.deny_unknown_fields, result.deny_unknown_fields);
    }

    #[test]
    fn test_serde_rename_rule_from_str() {
        for (s, _) in RENAME_RULES {
            s.parse::<RenameRule>().unwrap();
        }
    }

    #[test]
    fn test_serde_parse_container_rename_all() {
        let rename_all_attribute: syn::Attribute = parse_quote! {
            #[serde(rename_all = "camelCase")]
        };
        let attributes: &[Attribute] = &[rename_all_attribute];

        let result = parse_container(attributes).unwrap();
        assert_eq!(result.rename_all, Some(RenameRule::CamelCase));
    }

    #[test]
    fn test_serde_parse_container_untagged() {
        let untagged_attribute: syn::Attribute = parse_quote! {
            #[serde(untagged)]
        };
        let attributes: &[Attribute] = &[untagged_attribute];

        let result = parse_container(attributes).unwrap();
        assert_eq!(result.enum_repr, super::SerdeEnumRepr::Untagged);
    }

    #[test]
    fn test_serde_parse_container_internally_tagged() {
        let tag_attribute: syn::Attribute = parse_quote! {
            #[serde(tag = "t")]
        };
        let attributes: &[Attribute] = &[tag_attribute];

        let result = parse_container(attributes).unwrap();
        assert_eq!(
            result.enum_repr,
            super::SerdeEnumRepr::InternallyTagged {
                tag: "t".to_string()
            }
        );
    }

    #[test]
    fn test_serde_parse_container_adjacently_tagged() {
        let tag_attribute: syn::Attribute = parse_quote! {
            #[serde(tag = "t", content = "c")]
        };
        let attributes: &[Attribute] = &[tag_attribute];

        let result = parse_container(attributes).unwrap();
        assert_eq!(
            result.enum_repr,
            super::SerdeEnumRepr::AdjacentlyTagged {
                tag: "t".to_string(),
                content: "c".to_string()
            }
        );
    }
}
