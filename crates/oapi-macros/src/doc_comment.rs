use std::ops::Deref;

use syn::{Attribute, Expr, Lit, Meta};

const DOC_ATTRIBUTE_TYPE: &str = "doc";

/// CommentAttributes holds Vec of parsed doc comments
#[derive(Debug)]
pub(crate) struct CommentAttributes(pub(crate) Vec<String>);

impl CommentAttributes {
    /// Creates new [`CommentAttributes`] instance from [`Attribute`] slice filtering out all
    /// other attributes which are not `doc` comments
    pub(crate) fn from_attributes(attributes: &[Attribute]) -> Self {
        Self(Self::as_string_vec(
            attributes
                .iter()
                .filter(|attr| Self::is_doc_attribute(attr)),
        ))
    }

    fn is_doc_attribute(attr: &Attribute) -> bool {
        attr.path().is_ident(DOC_ATTRIBUTE_TYPE)
    }

    fn as_string_vec<'a, I: Iterator<Item = &'a Attribute>>(attrs: I) -> Vec<String> {
        attrs
            .into_iter()
            .filter_map(Self::parse_doc_comment)
            .collect()
    }

    fn parse_doc_comment(attr: &Attribute) -> Option<String> {
        match &attr.meta {
            Meta::NameValue(name_value) => {
                if let Expr::Lit(ref doc_comment) = name_value.value {
                    if let Lit::Str(ref comment) = doc_comment.lit {
                        Some(comment.value().trim().to_owned())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            // ignore `#[doc(hidden)]` and similar tags.
            _ => None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn parse_attrs(tokens: proc_macro2::TokenStream) -> Vec<Attribute> {
        let item: syn::ItemStruct = syn::parse2(tokens).unwrap();
        item.attrs
    }

    #[test]
    fn test_comment_attributes_empty() {
        let attrs = parse_attrs(quote! {
            struct Foo;
        });
        let comments = CommentAttributes::from_attributes(&attrs);
        assert!(comments.is_empty());
    }

    #[test]
    fn test_comment_attributes_single_doc() {
        let attrs = parse_attrs(quote! {
            /// This is a doc comment
            struct Foo;
        });
        let comments = CommentAttributes::from_attributes(&attrs);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0], "This is a doc comment");
    }

    #[test]
    fn test_comment_attributes_multiple_docs() {
        let attrs = parse_attrs(quote! {
            /// First line
            /// Second line
            /// Third line
            struct Foo;
        });
        let comments = CommentAttributes::from_attributes(&attrs);
        assert_eq!(comments.len(), 3);
        assert_eq!(comments[0], "First line");
        assert_eq!(comments[1], "Second line");
        assert_eq!(comments[2], "Third line");
    }

    #[test]
    fn test_comment_attributes_trims_whitespace() {
        let attrs = parse_attrs(quote! {
            ///   Padded with spaces
            struct Foo;
        });
        let comments = CommentAttributes::from_attributes(&attrs);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0], "Padded with spaces");
    }

    #[test]
    fn test_comment_attributes_filters_non_doc() {
        let attrs = parse_attrs(quote! {
            /// Doc comment
            #[derive(Debug)]
            #[allow(dead_code)]
            struct Foo;
        });
        let comments = CommentAttributes::from_attributes(&attrs);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0], "Doc comment");
    }

    #[test]
    fn test_comment_attributes_as_formatted_string() {
        let attrs = parse_attrs(quote! {
            /// First line
            /// Second line
            struct Foo;
        });
        let comments = CommentAttributes::from_attributes(&attrs);
        let formatted = comments.as_formatted_string();
        assert_eq!(formatted, "First line\nSecond line");
    }

    #[test]
    fn test_comment_attributes_deref() {
        let attrs = parse_attrs(quote! {
            /// Test
            struct Foo;
        });
        let comments = CommentAttributes::from_attributes(&attrs);
        // Test that we can use Vec methods through Deref
        let first: Option<&String> = comments.first();
        assert_eq!(first.map(|s| s.as_str()), Some("Test"));
    }

    #[test]
    fn test_comment_attributes_debug() {
        let attrs = parse_attrs(quote! {
            /// Test
            struct Foo;
        });
        let comments = CommentAttributes::from_attributes(&attrs);
        let debug_str = format!("{:?}", comments);
        assert!(debug_str.contains("CommentAttributes"));
    }
}
