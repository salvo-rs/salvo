use proc_macro2::Ident;
use syn::punctuated::Punctuated;
use syn::Expr;
use syn::{parenthesized, parse::Parse, Token};

use crate::operation::{parameter::Parameter, request_body::RequestBodyAttr, Response};
use crate::{parse_utils, security_requirement::SecurityRequirementAttr, Array};

#[derive(Default, Debug)]
pub struct EndpointAttr<'p> {
    pub request_body: Option<RequestBodyAttr<'p>>,
    pub responses: Vec<Response<'p>>,
    pub operation_id: Option<Expr>,
    pub tags: Option<Vec<Expr>>,
    pub parameters: Vec<Parameter<'p>>,
    pub(crate) security: Option<Array<'p, SecurityRequirementAttr>>,

    pub doc_comments: Option<Vec<String>>,
    pub deprecated: Option<bool>,
}

impl Parse for EndpointAttr<'_> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        const EXPECTED_ATTRIBUTE_MESSAGE: &str = "unexpected identifier, expected any of: operation_id, path, get, post, put, delete, options, head, patch, trace, connect, request_body, responses, params, tag, security, context_path";
        let mut attr = EndpointAttr::default();

        while !input.is_empty() {
            let ident = input
                .parse::<Ident>()
                .map_err(|error| syn::Error::new(error.span(), format!("{EXPECTED_ATTRIBUTE_MESSAGE}, {error}")))?;
            match &*ident.to_string() {
                "operation_id" => {
                    attr.operation_id = Some(parse_utils::parse_next(input, || Expr::parse(input))?);
                }
                "request_body" => {
                    attr.request_body = Some(input.parse::<RequestBodyAttr>()?);
                }
                "responses" => {
                    let responses;
                    parenthesized!(responses in input);
                    attr.responses = Punctuated::<Response, Token![,]>::parse_terminated(&responses)
                        .map(|punctuated| punctuated.into_iter().collect::<Vec<Response>>())?;
                }
                "parameters" => {
                    let parameters;
                    parenthesized!(parameters in input);
                    attr.parameters = Punctuated::<Parameter, Token![,]>::parse_terminated(&parameters)
                        .map(|punctuated| punctuated.into_iter().collect::<Vec<Parameter>>())?;
                }
                "tags" => {
                    let tags;
                    parenthesized!(tags in input);
                    attr.tags = Some(
                        Punctuated::<Expr, Token![,]>::parse_terminated(&tags)
                            .map(|punctuated| punctuated.into_iter().collect::<Vec<Expr>>())?,
                    );
                }
                "security" => {
                    let security;
                    parenthesized!(security in input);
                    attr.security = Some(parse_utils::parse_groups(&security)?)
                }
                _ => {
                    return Err(syn::Error::new(ident.span(), EXPECTED_ATTRIBUTE_MESSAGE));
                }
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(attr)
    }
}
