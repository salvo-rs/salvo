use std::fmt::Display;

use proc_macro2::{Group, Ident, TokenStream};
use quote::ToTokens;
use syn::{
    Expr, LitBool, LitStr, Path, Token, parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

#[derive(Clone, Debug)]
pub(crate) enum LitStrOrExpr {
    LitStr(LitStr),
    Expr(Expr),
}

impl LitStrOrExpr {
    pub(crate) fn is_empty(&self) -> bool {
        matches!(self, Self::LitStr(s) if s.value().is_empty())
    }
}

impl From<String> for LitStrOrExpr {
    fn from(value: String) -> Self {
        Self::LitStr(LitStr::new(&value, proc_macro2::Span::call_site()))
    }
}

impl Default for LitStrOrExpr {
    fn default() -> Self {
        Self::LitStr(LitStr::new("", proc_macro2::Span::call_site()))
    }
}

impl Parse for LitStrOrExpr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(LitStr) {
            Ok::<LitStrOrExpr, syn::Error>(LitStrOrExpr::LitStr(input.parse::<LitStr>()?))
        } else {
            Ok(LitStrOrExpr::Expr(input.parse::<Expr>()?))
        }
    }
}

impl ToTokens for LitStrOrExpr {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::LitStr(str) => str.to_tokens(tokens),
            Self::Expr(expr) => expr.to_tokens(tokens),
        }
    }
}

impl Display for LitStrOrExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LitStr(str) => write!(f, "{str}", str = str.value()),
            Self::Expr(expr) => write!(f, "{expr}", expr = expr.into_token_stream()),
        }
    }
}

pub(crate) fn parse_next<T: Sized>(
    input: ParseStream,
    next: impl FnOnce() -> syn::Result<T>,
) -> syn::Result<T> {
    input.parse::<Token![=]>()?;
    next()
}

pub(crate) fn parse_next_path_or_lit_str(input: ParseStream) -> syn::Result<String> {
    parse_next(input, || parse_path_or_lit_str(input))
}
pub(crate) fn parse_next_lit_str(input: ParseStream) -> syn::Result<String> {
    Ok(parse_next(input, || input.parse::<LitStr>())?.value())
}

pub(crate) fn parse_next_lit_str_or_expr(input: ParseStream) -> syn::Result<LitStrOrExpr> {
    parse_next(input, || LitStrOrExpr::parse(input)).map_err(|error| {
        syn::Error::new(
            error.span(),
            format!("expected literal string or expression argument: {error}"),
        )
    })
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

pub(crate) fn parse_punctuated_within_parenthesis<T>(
    input: ParseStream,
) -> syn::Result<Punctuated<T, Token![,]>>
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
                return Err(syn::Error::new(
                    ident.span(),
                    format!("unexpected token {ident}, expected: json!(...)"),
                ));
            }

            Ok(ident)
        })?;
        input.parse::<Token![!]>()?;

        Ok(input.parse::<Group>()?.stream())
    } else {
        Err(syn::Error::new(
            input.span(),
            "unexpected token, expected json!(...)",
        ))
    }
}

pub(crate) fn parse_path_or_lit_str(input: ParseStream) -> syn::Result<String> {
    if let Ok(path) = input.parse::<Path>() {
        Ok(path.to_token_stream().to_string())
    } else if let Ok(lit) = input.parse::<LitStr>() {
        Ok(lit.value())
    } else {
        Err(syn::Error::new(input.span(), "invalid indent or lit str"))
    }
}
