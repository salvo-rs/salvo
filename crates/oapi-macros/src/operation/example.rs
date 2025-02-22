use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, quote};
use syn::parse::{Parse, ParseStream};
use syn::{Error, LitStr, Token, parenthesized};

use crate::{AnyValue, parse_utils};

// (name = (summary = "...", description = "...", value = "..", external_value = "..."))
#[derive(Default, Debug)]
pub(crate) struct Example {
    pub(crate) name: String,
    pub(crate) summary: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) value: Option<AnyValue>,
    pub(crate) external_value: Option<String>,
}

impl Parse for Example {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let example_stream;
        parenthesized!(example_stream in input);
        let mut example = Example {
            name: example_stream.parse::<LitStr>()?.value(),
            ..Default::default()
        };
        example_stream.parse::<Token![=]>()?;

        let content;
        parenthesized!(content in example_stream);

        while !content.is_empty() {
            let ident = content.parse::<Ident>()?;
            let attr_name = &*ident.to_string();
            match attr_name {
                "summary" => {
                    example.summary = Some(
                        parse_utils::parse_next(&content, || content.parse::<LitStr>())?.value(),
                    )
                }
                "description" => {
                    example.description = Some(
                        parse_utils::parse_next(&content, || content.parse::<LitStr>())?.value(),
                    )
                }
                "value" => {
                    example.value = Some(parse_utils::parse_next(&content, || {
                        AnyValue::parse_json(&content)
                    })?)
                }
                "external_value" => {
                    example.external_value = Some(
                        parse_utils::parse_next(&content, || content.parse::<LitStr>())?.value(),
                    )
                }
                _ => {
                    return Err(Error::new(
                        ident.span(),
                        format!(
                            "unexpected attribute: {attr_name}, expected one of: summary, description, value, external_value"
                        ),
                    ));
                }
            }

            if !content.is_empty() {
                content.parse::<Token![,]>()?;
            }
        }

        Ok(example)
    }
}

impl ToTokens for Example {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        let summary = self
            .summary
            .as_ref()
            .map(|summary| quote!(.summary(#summary)));
        let description = self
            .description
            .as_ref()
            .map(|description| quote!(.description(#description)));
        let value = self.value.as_ref().map(|value| quote!(.value(#value)));
        let external_value = self
            .external_value
            .as_ref()
            .map(|external_value| quote!(.external_value(#external_value)));

        tokens.extend(quote! {
            #oapi::oapi::Example::new()
                #summary
                #description
                #value
                #external_value
        })
    }
}
