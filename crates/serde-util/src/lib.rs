//! Provides serde related features parsing serde attributes from types.
use proc_macro2::{Ident, Span, TokenTree};
use proc_macro_error::abort;
use syn::{buffer::Cursor, Attribute, Error};

pub(crate) mod case;
pub use case::RenameRule;

trait ResultExt<T> {
    fn unwrap_or_abort(self) -> T;
    fn expect_or_abort(self, message: &str) -> T;
}

impl<T> ResultExt<T> for Result<T, syn::Error> {
    fn unwrap_or_abort(self) -> T {
        match self {
            Ok(value) => value,
            Err(error) => abort!(error.span(), format!("{error}")),
        }
    }

    fn expect_or_abort(self, message: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => abort!(error.span(), format!("{error}: {message}")),
        }
    }
}

#[inline]
fn parse_next_lit_str(next: Cursor) -> Option<(String, Span)> {
    match next.token_tree() {
        Some((tt, next)) => match tt {
            TokenTree::Punct(punct) if punct.as_char() == '=' => parse_next_lit_str(next),
            TokenTree::Literal(literal) => Some((literal.to_string().replace('\"', ""), literal.span())),
            _ => None,
        },
        _ => None,
    }
}

#[derive(Default, Debug)]
pub struct SerdeValue {
    pub skip: bool,
    pub rename: Option<String>,
    pub is_default: bool,
    pub flatten: bool,
    pub skip_serializing_if: bool,
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
                        if ident == "skip" || ident == "skip_serializing" || ident == "skip_deserializing" =>
                    {
                        value.skip = true
                    }
                    TokenTree::Ident(ident) if ident == "skip_serializing_if" => value.skip_serializing_if = true,
                    TokenTree::Ident(ident) if ident == "flatten" => value.flatten = true,
                    TokenTree::Ident(ident) if ident == "rename" => {
                        if let Some((literal, _)) = parse_next_lit_str(next) {
                            value.rename = Some(literal)
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
    #[default]
    ExternallyTagged,
    InternallyTagged {
        tag: String,
    },
    AdjacentlyTagged {
        tag: String,
        content: String,
    },
    Untagged,
    /// This is a variant that can never happen because `serde` will not accept it.
    /// With the current implementation it is necessary to have it as an intermediate state when parsing the
    /// attributes
    UnfinishedAdjacentlyTagged {
        content: String,
    },
}

/// Attributes defined within a `#[serde(...)]` container attribute.
#[derive(Default, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct SerdeContainer {
    pub rename_all: Option<RenameRule>,
    pub enum_repr: SerdeEnumRepr,
    pub is_default: bool,
    pub deny_unknown_fields: bool,
}

impl SerdeContainer {
    /// Parse a single serde attribute, currently supported attributes are:
    ///     * `rename_all = ...`
    ///     * `tag = ...`
    ///     * `content = ...`
    ///     * `untagged = ...`
    ///     * `default = ...`
    fn parse_attribute(&mut self, ident: Ident, next: Cursor) -> syn::Result<()> {
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
                        SerdeEnumRepr::ExternallyTagged => SerdeEnumRepr::InternallyTagged { tag: literal },
                        SerdeEnumRepr::UnfinishedAdjacentlyTagged { content } => SerdeEnumRepr::AdjacentlyTagged {
                            tag: literal,
                            content: content.clone(),
                        },
                        SerdeEnumRepr::InternallyTagged { .. } | SerdeEnumRepr::AdjacentlyTagged { .. } => {
                            abort!(span, "Duplicate serde tag argument")
                        }
                        SerdeEnumRepr::Untagged => abort!(span, "Untagged enum cannot have tag"),
                    };
                }
            }
            "content" => {
                if let Some((literal, span)) = parse_next_lit_str(next) {
                    self.enum_repr = match &self.enum_repr {
                        SerdeEnumRepr::InternallyTagged { tag } => SerdeEnumRepr::AdjacentlyTagged {
                            tag: tag.clone(),
                            content: literal,
                        },
                        SerdeEnumRepr::ExternallyTagged => {
                            SerdeEnumRepr::UnfinishedAdjacentlyTagged { content: literal }
                        }
                        SerdeEnumRepr::AdjacentlyTagged { .. } | SerdeEnumRepr::UnfinishedAdjacentlyTagged { .. } => {
                            abort!(span, "Duplicate serde content argument")
                        }
                        SerdeEnumRepr::Untagged => {
                            abort!(span, "Untagged enum cannot have content")
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
                    container.parse_attribute(ident, next)?
                }

                rest = next;
            }
            Ok(((), rest))
        })?;

        Ok(container)
    }
}

pub fn parse_value(attributes: &[Attribute]) -> Option<SerdeValue> {
    attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("serde"))
        .map(|serde_attribute| serde_attribute.parse_args_with(SerdeValue::parse).unwrap_or_abort())
        .try_fold(SerdeValue::default(), |mut acc, value| {
            if value.skip {
                acc.skip = value.skip;
            }
            if value.skip_serializing_if {
                acc.skip_serializing_if = value.skip_serializing_if;
            }
            if value.rename.is_some() {
                acc.rename = value.rename;
            }
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

pub fn parse_container(attributes: &[Attribute]) -> Option<SerdeContainer> {
    attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("serde"))
        .map(|serde_attribute| serde_attribute.parse_args_with(SerdeContainer::parse).unwrap_or_abort())
        .try_fold(SerdeContainer::default(), |mut acc, value| {
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
    use super::{case::RENAME_RULES, parse_container, RenameRule, SerdeContainer};
    use syn::{parse_quote, Attribute};

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
        assert_eq!(expected, result);
    }

    #[test]
    fn test_serde_rename_rule_from_str() {
        for (s, _) in RENAME_RULES {
            s.parse::<RenameRule>().unwrap();
        }
    }
}
