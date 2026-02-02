use std::fmt::{self, Debug, Display, Formatter};

use proc_macro2::{Group, Ident, TokenStream};
use quote::ToTokens;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Expr, LitBool, LitStr, Path, Token, parenthesized};

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
            Ok::<Self, syn::Error>(Self::LitStr(input.parse::<LitStr>()?))
        } else {
            Ok(Self::Expr(input.parse::<Expr>()?))
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
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
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

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_lit_str_or_expr_from_string() {
        let result = LitStrOrExpr::from("test".to_string());
        assert!(matches!(result, LitStrOrExpr::LitStr(_)));
    }

    #[test]
    fn test_lit_str_or_expr_default() {
        let result = LitStrOrExpr::default();
        assert!(result.is_empty());
    }

    #[test]
    fn test_lit_str_or_expr_is_empty_true() {
        let result = LitStrOrExpr::from("".to_string());
        assert!(result.is_empty());
    }

    #[test]
    fn test_lit_str_or_expr_is_empty_false() {
        let result = LitStrOrExpr::from("not empty".to_string());
        assert!(!result.is_empty());
    }

    #[test]
    fn test_lit_str_or_expr_parse_lit_str() {
        let result: LitStrOrExpr = syn::parse_str(r#""hello world""#).unwrap();
        assert!(matches!(result, LitStrOrExpr::LitStr(_)));
    }

    #[test]
    fn test_lit_str_or_expr_parse_expr() {
        let result: LitStrOrExpr = syn::parse_str("some_variable").unwrap();
        assert!(matches!(result, LitStrOrExpr::Expr(_)));
    }

    #[test]
    fn test_lit_str_or_expr_to_tokens() {
        let lit = LitStrOrExpr::from("test".to_string());
        let mut tokens = TokenStream::new();
        lit.to_tokens(&mut tokens);
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_lit_str_or_expr_display_lit_str() {
        let lit = LitStrOrExpr::from("display test".to_string());
        let display = format!("{}", lit);
        assert_eq!(display, "display test");
    }

    #[test]
    fn test_lit_str_or_expr_display_expr() {
        let result: LitStrOrExpr = syn::parse_str("my_var").unwrap();
        let display = format!("{}", result);
        assert!(display.contains("my_var"));
    }

    #[test]
    fn test_lit_str_or_expr_debug() {
        let lit = LitStrOrExpr::from("test".to_string());
        let debug = format!("{:?}", lit);
        assert!(debug.contains("LitStr"));
    }

    #[test]
    fn test_lit_str_or_expr_clone() {
        let original = LitStrOrExpr::from("clone test".to_string());
        let cloned = original.clone();
        assert!(matches!(cloned, LitStrOrExpr::LitStr(_)));
    }

    #[test]
    fn test_parse_bool_or_true_no_value() {
        let tokens = quote! {};
        let result: bool = syn::parse::Parser::parse2(parse_bool_or_true, tokens).unwrap();
        assert!(result);
    }

    #[test]
    fn test_parse_bool_or_true_with_true() {
        let tokens = quote! { = true };
        let result: bool = syn::parse::Parser::parse2(parse_bool_or_true, tokens).unwrap();
        assert!(result);
    }

    #[test]
    fn test_parse_bool_or_true_with_false() {
        let tokens = quote! { = false };
        let result: bool = syn::parse::Parser::parse2(parse_bool_or_true, tokens).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_parse_path_or_lit_str_path() {
        let tokens = quote! { std::string::String };
        let result: String =
            syn::parse::Parser::parse2(parse_path_or_lit_str, tokens).unwrap();
        assert!(result.contains("String"));
    }

    #[test]
    fn test_parse_path_or_lit_str_lit() {
        let tokens = quote! { "literal string" };
        let result: String =
            syn::parse::Parser::parse2(parse_path_or_lit_str, tokens).unwrap();
        assert_eq!(result, "literal string");
    }

    #[test]
    fn test_parse_json_token_stream_valid() {
        let tokens = quote! { json!({ "key": "value" }) };
        let result = syn::parse::Parser::parse2(parse_json_token_stream, tokens);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_json_token_stream_invalid_ident() {
        let tokens = quote! { notjson!({ "key": "value" }) };
        let result = syn::parse::Parser::parse2(parse_json_token_stream, tokens);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_next_lit_str() {
        let tokens = quote! { = "test string" };
        let result: String =
            syn::parse::Parser::parse2(parse_next_lit_str, tokens).unwrap();
        assert_eq!(result, "test string");
    }
}
