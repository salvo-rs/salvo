use std::ops::Deref;

use proc_macro2::Ident;
use proc_macro_error::abort_call_site;
use syn::{Attribute, Expr, Lit, Meta};

const DOC_ATTRIBUTE_TYPE: &str = "doc";

/// CommentAttributes holds Vec of parsed doc comments
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct CommentAttributes(pub(crate) Vec<String>);

impl CommentAttributes {
    /// Creates new [`CommentAttributes`] instance from [`Attribute`] slice filtering out all
    /// other attributes which are not `doc` comments
    pub(crate) fn from_attributes(attributes: &[Attribute]) -> Self {
        Self(Self::as_string_vec(
            attributes.iter().filter(Self::is_doc_attribute),
        ))
    }

    fn is_doc_attribute(attribute: &&Attribute) -> bool {
        match Self::get_attribute_ident(attribute) {
            Some(attribute) => attribute == DOC_ATTRIBUTE_TYPE,
            None => false,
        }
    }

    fn get_attribute_ident(attribute: &Attribute) -> Option<&Ident> {
        attribute.path().get_ident()
    }

    fn as_string_vec<'a, I: Iterator<Item = &'a Attribute>>(attributes: I) -> Vec<String> {
        attributes
            .into_iter()
            .filter_map(Self::parse_doc_comment)
            .collect()
    }

    fn parse_doc_comment(attribute: &Attribute) -> Option<String> {
        match &attribute.meta {
            Meta::NameValue(name_value) => {
                if let Expr::Lit(ref doc_comment) = name_value.value {
                    if let Lit::Str(ref comment) = doc_comment.lit {
                        Some(comment.value().trim().to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => abort_call_site!("Expected only Meta::NameValue type"),
        }
    }

    /// Returns found `doc comments` as formatted `String` joining them all with `\n` _(new line)_.
    pub(crate) fn as_formatted_string(&self) -> String {
        self.join("\n")
    }
}

impl Deref for CommentAttributes {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
