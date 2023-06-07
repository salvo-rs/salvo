use std::ops::Deref;

use proc_macro_error::abort_call_site;
use syn::{Attribute, Expr, Lit, Meta};

const DOC_ATTRIBUTE_TYPE: &str = "doc";

/// CommentAttributes holds Vec of parsed doc comments
#[derive(Debug)]
pub(crate) struct CommentAttributes(pub(crate) Vec<String>);

impl CommentAttributes {
    /// Creates new [`CommentAttributes`] instance from [`Attribute`] slice filtering out all
    /// other attributes which are not `doc` comments
    pub(crate) fn from_attributes(attributes: &[Attribute]) -> Self {
        Self(Self::as_string_vec(attributes.iter().filter(Self::is_doc_attribute)))
    }

    fn is_doc_attribute(attr: &&Attribute) -> bool {
        attr.path().is_ident(DOC_ATTRIBUTE_TYPE)
    }

    fn as_string_vec<'a, I: Iterator<Item = &'a Attribute>>(attrs: I) -> Vec<String> {
        attrs.into_iter().filter_map(Self::parse_doc_comment).collect()
    }

    fn parse_doc_comment(attr: &Attribute) -> Option<String> {
        match &attr.meta {
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
