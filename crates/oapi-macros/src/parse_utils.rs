use proc_macro2::{Group, Ident, TokenStream};
use syn::{
    parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    Error, LitBool, LitStr, Token,
};

use crate::ResultExt;

pub(crate) fn parse_next<T: Sized>(input: ParseStream, next: impl FnOnce() -> T) -> T {
    input
        .parse::<Token![=]>()
        .expect_or_abort("expected equals token before value assignment");
    next()
}

pub(crate) fn parse_next_literal_str(input: ParseStream) -> syn::Result<String> {
    Ok(parse_next(input, || input.parse::<LitStr>())?.value())
}

pub(crate) fn parse_groups<T, R>(input: ParseStream) -> syn::Result<R>
where
    T: Sized,
    T: Parse,
    R: FromIterator<T>,
{
    Punctuated::<Group, Token![,]>::parse_terminated(input).and_then(|groups| {
        groups
            .into_iter()
            .map(|group| syn::parse2::<T>(group.stream()))
            .collect::<syn::Result<R>>()
    })
}

pub(crate) fn parse_punctuated_within_parenthesis<T>(input: ParseStream) -> syn::Result<Punctuated<T, Token![,]>>
where
    T: Parse,
{
    let content;
    parenthesized!(content in input);
    Punctuated::<T, Token![,]>::parse_terminated(&content)
}

pub(crate) fn parse_bool_or_true(input: ParseStream) -> syn::Result<bool> {
    if input.peek(Token![=]) && input.peek2(LitBool) {
        input.parse::<Token![=]>()?;

        Ok(input.parse::<LitBool>()?.value())
    } else {
        Ok(true)
    }
}

/// Parse `json!(...)` as a [`TokenStream`].
pub(crate) fn parse_json_token_stream(input: ParseStream) -> syn::Result<TokenStream> {
    if input.peek(syn::Ident) && input.peek2(Token![!]) {
        input.parse::<Ident>().and_then(|ident| {
            if ident != "json" {
                return Err(Error::new(
                    ident.span(),
                    format!("unexpected token {ident}, expected: json!(...)"),
                ));
            }

            Ok(ident)
        })?;
        input.parse::<Token![!]>()?;

        Ok(input.parse::<Group>()?.stream())
    } else {
        Err(Error::new(input.span(), "unexpected token, expected json!(...)"))
    }
}
