use quote::{quote, ToTokens};
use syn::{
    bracketed,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    token::Comma,
    LitStr, Token,
};

use crate::Array;

#[derive(Default, Debug)]
pub(crate) struct SecurityRequirementsAttrItem {
    pub(crate) name: Option<String>,
    pub(crate) scopes: Option<Vec<String>>,
}

#[derive(Default, Debug)]
#[cfg_attr(feature = "debug", derive(Debug))]
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
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        let oapi = crate::oapi_crate();
        stream.extend(quote! {
            #oapi::oapi::security::SecurityRequirement::default()
        });

        for requirement in &self.0 {
            if let (Some(name), Some(scopes)) = (&requirement.name, &requirement.scopes) {
                let scopes = scopes.iter().collect::<Array<&String>>();
                let scopes_len = scopes.len();

                stream.extend(quote! {
                    .add::<&str, [&str; #scopes_len], &str>(#name, #scopes)
                });
            }
        }
    }
}
