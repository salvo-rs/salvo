use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    LitStr, Token, bracketed,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    token::Comma,
};

use crate::Array;

#[derive(Default, Debug)]
pub(crate) struct SecurityRequirementsAttrItem {
    pub(crate) name: Option<String>,
    pub(crate) scopes: Option<Vec<String>>,
}

#[derive(Default, Debug)]
pub(crate) struct SecurityRequirementsAttr(Punctuated<SecurityRequirementsAttrItem, Comma>);

impl Parse for SecurityRequirementsAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Punctuated::<SecurityRequirementsAttrItem, Comma>::parse_terminated(input)
            .map(|o| Self(o.into_iter().collect()))
    }
}

impl Parse for SecurityRequirementsAttrItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse::<LitStr>()?.value();
        input.parse::<Token![=]>()?;

        let scopes_stream;
        bracketed!(scopes_stream in input);
        let scopes = Punctuated::<LitStr, Token![,]>::parse_terminated(&scopes_stream)?
            .iter()
            .map(LitStr::value)
            .collect::<Vec<_>>();

        Ok(Self {
            name: Some(name),
            scopes: Some(scopes),
        })
    }
}

impl ToTokens for SecurityRequirementsAttr {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        tokens.extend(quote! {
            #oapi::oapi::security::SecurityRequirement::default()
        });

        for requirement in &self.0 {
            if let (Some(name), Some(scopes)) = (&requirement.name, &requirement.scopes) {
                let scopes = scopes.iter().collect::<Array<&String>>();
                let scopes_len = scopes.len();

                tokens.extend(quote! {
                    .add::<&str, [&str; #scopes_len], &str>(#name, #scopes)
                });
            }
        }
    }
}
