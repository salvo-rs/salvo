use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::parse::Parse;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{Ident, Token};

use crate::{AnyValue, Server, parse_utils};

/// ("name" = (link))
#[derive(Debug)]
pub(crate) struct LinkTuple(pub(crate) parse_utils::LitStrOrExpr, pub(crate) Link);

impl Parse for LinkTuple {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let inner;
        syn::parenthesized!(inner in input);

        let name = inner.parse::<parse_utils::LitStrOrExpr>()?;
        inner.parse::<Token![=]>()?;
        let value = inner.parse::<Link>()?;

        Ok(LinkTuple(name, value))
    }
}

/// (operation_ref = "", operation_id = "",
///     parameters(
///          ("name" = value),
///          ("name" = value)
///     ),
///     request_body = value,
///     description = "",
///     server(...)
/// )
#[derive(Default, Debug)]
pub(crate) struct Link {
    operation_ref: Option<parse_utils::LitStrOrExpr>,
    operation_id: Option<parse_utils::LitStrOrExpr>,
    parameters: Punctuated<LinkParameter, Comma>,
    request_body: Option<AnyValue>,
    description: Option<parse_utils::LitStrOrExpr>,
    server: Option<Server>,
}

impl Parse for Link {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let inner;
        syn::parenthesized!(inner in input);
        let mut link = Link::default();

        while !inner.is_empty() {
            let ident = inner.parse::<Ident>()?;
            let attribute = &*ident.to_string();

            match attribute {
                "operation_ref" => {
                    link.operation_ref = Some(parse_utils::parse_next_lit_str_or_expr(&inner)?)
                }
                "operation_id" => {
                    link.operation_id = Some(parse_utils::parse_next_lit_str_or_expr(&inner)?)
                }
                "parameters" => {
                    link.parameters = parse_utils::parse_punctuated_within_parenthesis(&inner)?;
                }
                "request_body" => {
                    link.request_body = Some(parse_utils::parse_next(&inner, || {
                        AnyValue::parse_any(&inner)
                    })?)
                }
                "description" => {
                    link.description = Some(parse_utils::parse_next_lit_str_or_expr(&inner)?)
                }
                "server" => link.server = Some(inner.call(Server::parse)?),
                _ => {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!(
                            "unexpected attribute: {attribute}, expected any of: operation_ref, operation_id, parameters, request_body, description, server"
                        ),
                    ));
                }
            }

            if !inner.is_empty() {
                inner.parse::<Token![,]>()?;
            }
        }

        Ok(link)
    }
}

impl ToTokens for Link {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let operation_ref = self
            .operation_ref
            .as_ref()
            .map(|operation_ref| quote! { .operation_ref(#operation_ref)});

        let operation_id = self
            .operation_id
            .as_ref()
            .map(|operation_id| quote! { .operation_id(#operation_id)});

        let parameters =
            self.parameters
                .iter()
                .fold(TokenStream::new(), |mut params, parameter| {
                    let name = &parameter.name;
                    let value = &parameter.value;
                    params.extend(quote! { .parameter(#name, #value) });

                    params
                });

        let request_body = self
            .request_body
            .as_ref()
            .map(|request_body| quote! { .request_body(Some(#request_body)) });

        let description = self
            .description
            .as_ref()
            .map(|description| quote! { .description(#description) });

        let server = self
            .server
            .as_ref()
            .map(|server| quote! { .server(Some(#server)) });

        tokens.extend(quote! {
            utoipa::openapi::link::Link::builder()
                #operation_ref
                #operation_id
                #parameters
                #request_body
                #description
                #server
                .build()
        })
    }
}

/// ("foobar" = json!(...))
#[derive(Debug)]
pub(crate) struct LinkParameter {
    name: parse_utils::LitStrOrExpr,
    value: parse_utils::LitStrOrExpr,
}

impl Parse for LinkParameter {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let inner;
        syn::parenthesized!(inner in input);
        let name = inner.parse::<parse_utils::LitStrOrExpr>()?;

        inner.parse::<Token![=]>()?;

        let value = inner.parse::<parse_utils::LitStrOrExpr>()?;

        Ok(LinkParameter { name, value })
    }
}
