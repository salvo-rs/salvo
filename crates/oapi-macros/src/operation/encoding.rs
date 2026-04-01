use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, quote};
use syn::parse::{Parse, ParseStream};
use syn::{Error, Token, parenthesized};

use crate::parse_utils;

/// Parsed encoding attributes for a single field in a multipart request body.
///
/// Supports the following attributes inside parentheses:
///   * `content_type = "..."` - The Content-Type for encoding a specific property.
///   * `explode = true/false` - Whether array/object values generate separate parameters.
///   * `allow_reserved = true/false` - Whether reserved characters are allowed without encoding.
#[derive(Default, Debug)]
pub(crate) struct Encoding {
    content_type: Option<parse_utils::LitStrOrExpr>,
    explode: Option<bool>,
    allow_reserved: Option<bool>,
}

impl Parse for Encoding {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        parenthesized!(content in input);

        let mut encoding = Encoding::default();

        while !content.is_empty() {
            let ident = content.parse::<Ident>()?;
            let attribute_name = &*ident.to_string();
            match attribute_name {
                "content_type" => {
                    encoding.content_type =
                        Some(parse_utils::parse_next_lit_str_or_expr(&content)?);
                }
                "explode" => {
                    encoding.explode = Some(parse_utils::parse_bool_or_true(&content)?);
                }
                "allow_reserved" => {
                    encoding.allow_reserved = Some(parse_utils::parse_bool_or_true(&content)?);
                }
                _ => {
                    return Err(Error::new(
                        ident.span(),
                        format!(
                            "unexpected attribute: {attribute_name}, expected one of: content_type, explode, allow_reserved"
                        ),
                    ));
                }
            }

            if !content.is_empty() {
                content.parse::<Token![,]>()?;
            }
        }

        Ok(encoding)
    }
}

impl ToTokens for Encoding {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        let content_type = self
            .content_type
            .as_ref()
            .map(|ct| quote!(.content_type(#ct)));
        let explode = self.explode.map(|value| quote!(.explode(#value)));
        let allow_reserved = self
            .allow_reserved
            .map(|value| quote!(.allow_reserved(#value)));

        tokens.extend(quote! {
            #oapi::oapi::Encoding::default()
                #content_type
                #explode
                #allow_reserved
        });
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::*;

    #[test]
    fn test_parse_encoding_content_type_only() {
        let tokens = quote! { (content_type = "application/octet-stream") };
        let encoding: Encoding = syn::parse2(tokens).unwrap();
        assert!(encoding.content_type.is_some());
        assert!(encoding.explode.is_none());
        assert!(encoding.allow_reserved.is_none());
    }

    #[test]
    fn test_parse_encoding_all_fields() {
        let tokens = quote! { (content_type = "application/octet-stream", explode = true, allow_reserved = false) };
        let encoding: Encoding = syn::parse2(tokens).unwrap();
        assert!(encoding.content_type.is_some());
        assert_eq!(encoding.explode, Some(true));
        assert_eq!(encoding.allow_reserved, Some(false));
    }

    #[test]
    fn test_parse_encoding_explode_only() {
        let tokens = quote! { (explode) };
        let encoding: Encoding = syn::parse2(tokens).unwrap();
        assert!(encoding.content_type.is_none());
        assert_eq!(encoding.explode, Some(true));
    }

    #[test]
    fn test_parse_encoding_unknown_attribute() {
        let tokens = quote! { (unknown = "value") };
        let result: syn::Result<Encoding> = syn::parse2(tokens);
        assert!(result.is_err());
    }
}
