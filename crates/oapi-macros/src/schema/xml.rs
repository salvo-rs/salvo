use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, quote};
use syn::{Error, LitStr, Token, parenthesized, parse::Parse, token::Paren};

use crate::parse_utils;

#[derive(Default, Clone, Debug)]
pub(crate) struct XmlAttr {
    pub(crate) name: Option<String>,
    pub(crate) namespace: Option<String>,
    pub(crate) prefix: Option<String>,
    pub(crate) is_attribute: bool,
    pub(crate) is_wrapped: Option<Ident>,
    pub(crate) wrap_name: Option<String>,
}

impl XmlAttr {
    pub(crate) fn with_wrapped(is_wrapped: Option<Ident>, wrap_name: Option<String>) -> Self {
        Self {
            is_wrapped,
            wrap_name,
            ..Default::default()
        }
    }
}

impl Parse for XmlAttr {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        const EXPECTED_ATTRIBUTE_MESSAGE: &str =
            "unexpected attribute, expected any of: name, namespace, prefix, attribute, wrapped";
        let mut xml = XmlAttr::default();

        while !input.is_empty() {
            let attribute = input
                .parse::<Ident>()
                .map_err(|error| Error::new(error.span(), EXPECTED_ATTRIBUTE_MESSAGE))?;
            let attr_name = &*attribute.to_string();

            match attr_name {
                "name" => {
                    xml.name =
                        Some(parse_utils::parse_next(input, || input.parse::<LitStr>())?.value())
                }
                "namespace" => {
                    xml.namespace =
                        Some(parse_utils::parse_next(input, || input.parse::<LitStr>())?.value())
                }
                "prefix" => {
                    xml.prefix =
                        Some(parse_utils::parse_next(input, || input.parse::<LitStr>())?.value())
                }
                "attribute" => xml.is_attribute = parse_utils::parse_bool_or_true(input)?,
                "wrapped" => {
                    // wrapped or wrapped(name = "wrap_name")
                    if input.peek(Paren) {
                        let group;
                        parenthesized!(group in input);

                        let wrapped_attribute = group.parse::<Ident>().map_err(|error| {
                            Error::new(
                                error.span(),
                                format!("unexpected attribute, expected: name, {error}"),
                            )
                        })?;
                        if wrapped_attribute != "name" {
                            return Err(Error::new(
                                wrapped_attribute.span(),
                                "unexpected wrapped attribute, expected: name",
                            ));
                        }
                        group.parse::<Token![=]>()?;
                        xml.wrap_name = Some(group.parse::<LitStr>()?.value());
                    }
                    xml.is_wrapped = Some(attribute);
                }
                _ => return Err(Error::new(attribute.span(), EXPECTED_ATTRIBUTE_MESSAGE)),
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(xml)
    }
}

impl ToTokens for XmlAttr {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        tokens.extend(quote! {
            #oapi::oapi::Xml::new()
        });

        if let Some(ref name) = self.name {
            tokens.extend(quote! {
                .name(#name)
            })
        }

        if let Some(ref namespace) = self.namespace {
            tokens.extend(quote! {
                .namespace(#namespace)
            })
        }

        if let Some(ref prefix) = self.prefix {
            tokens.extend(quote! {
                .prefix(#prefix)
            })
        }

        if self.is_attribute {
            tokens.extend(quote! {
                .attribute(true)
            })
        }

        if self.is_wrapped.is_some() {
            tokens.extend(quote! {
                .wrapped(true)
            });

            // if is wrapped and wrap name is defined use wrap name instead
            if let Some(ref wrap_name) = self.wrap_name {
                tokens.extend(quote! {
                    .name(#wrap_name)
                })
            }
        }
    }
}
