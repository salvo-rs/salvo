use proc_macro2::TokenStream;
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
pub(crate) struct SecurityRequirementAttr {
    name: Option<String>,
    scopes: Option<Vec<String>>,
}

impl Parse for SecurityRequirementAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Self { ..Default::default() });
        }
        let name = input.parse::<LitStr>()?.value();
        input.parse::<Token![=]>()?;

        let scopes_stream;
        bracketed!(scopes_stream in input);
        let scopes = Punctuated::<LitStr, Comma>::parse_terminated(&scopes_stream)?
            .iter()
            .map(LitStr::value)
            .collect::<Vec<_>>();

        Ok(Self {
            name: Some(name),
            scopes: Some(scopes),
        })
    }
}

impl ToTokens for SecurityRequirementAttr {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        if let (Some(name), Some(scopes)) = (&self.name, &self.scopes) {
            let scopes_array = scopes.iter().collect::<Array<&String>>();
            let scopes_len = scopes.len();

            tokens.extend(quote! {
                #oapi::oapi::security::SecurityRequirement::new::<&str, [&str; #scopes_len], &str>(#name, #scopes_array)
            })
        } else {
            tokens.extend(quote! {
                #oapi::oapi::security::SecurityRequirement::default()
            })
        }
    }
}
