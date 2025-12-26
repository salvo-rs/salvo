use proc_macro2::Ident;
use syn::punctuated::Punctuated;
use syn::{parenthesized, parse::Parse, Expr};

use crate::operation::request_body::RequestBodyAttr;
use crate::{
    Array, Parameter, Response, Token, parse_utils, security_requirement::SecurityRequirementsAttr,
};

#[derive(Default, Debug)]
pub(crate) struct EndpointAttr<'p> {
    pub(crate) request_body: Option<RequestBodyAttr<'p>>,
    pub(crate) responses: Vec<Response<'p>>,
    pub(crate) status_codes: Vec<Expr>,
    pub(crate) operation_id: Option<Expr>,
    pub(crate) tags: Option<Vec<Expr>>,
    pub(crate) parameters: Vec<Parameter<'p>>,
    pub(crate) security: Option<Array<'p, SecurityRequirementsAttr>>,

    pub(crate) doc_comments: Option<Vec<String>>,
    pub(crate) deprecated: Option<bool>,
    pub(crate) description: Option<parse_utils::LitStrOrExpr>,
    pub(crate) summary: Option<parse_utils::LitStrOrExpr>,
}

impl Parse for EndpointAttr<'_> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        const EXPECTED_ATTRIBUTE_MESSAGE: &str = "unexpected identifier, expected any of: operation_id, path, get, post, put, delete, options, head, patch, trace, connect, request_body, responses, params, tag, security, context_path, description, summary";
        let mut attr = EndpointAttr::default();

        while !input.is_empty() {
            let ident = input.parse::<Ident>().map_err(|error| {
                syn::Error::new(
                    error.span(),
                    format!("{EXPECTED_ATTRIBUTE_MESSAGE}, {error}"),
                )
            })?;
            match &*ident.to_string() {
                "operation_id" => {
                    attr.operation_id =
                        Some(parse_utils::parse_next(input, || Expr::parse(input))?);
                }
                "request_body" => {
                    attr.request_body = Some(input.parse::<RequestBodyAttr>()?);
                }
                "responses" => {
                    let responses;
                    parenthesized!(responses in input);
                    attr.responses =
                        Punctuated::<Response, Token![,]>::parse_terminated(&responses)
                            .map(|punctuated| punctuated.into_iter().collect::<Vec<Response>>())?;
                }
                "status_codes" => {
                    let status_codes;
                    parenthesized!(status_codes in input);
                    attr.status_codes =
                        Punctuated::<Expr, Token![,]>::parse_terminated(&status_codes)
                            .map(|punctuated| punctuated.into_iter().collect::<Vec<Expr>>())?;
                }
                "parameters" => {
                    let parameters;
                    parenthesized!(parameters in input);
                    attr.parameters =
                        Punctuated::<Parameter, Token![,]>::parse_terminated(&parameters)
                            .map(|punctuated| punctuated.into_iter().collect::<Vec<Parameter>>())?;
                }
                "tags" => {
                    let tags;
                    parenthesized!(tags in input);
                    let parsed: Punctuated<Expr, Token![,]> =
                        Punctuated::<Expr, Token![,]>::parse_terminated(&tags)?;
                    attr.tags = Some(parsed.into_iter().collect());
                }
                "security" => {
                    let security;
                    parenthesized!(security in input);
                    attr.security = Some(parse_utils::parse_groups(&security)?)
                }
                "description" => {
                    attr.description = Some(parse_utils::parse_next_lit_str_or_expr(input)?)
                }
                "summary" => attr.summary = Some(parse_utils::parse_next_lit_str_or_expr(input)?),
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

#[cfg(test)]
mod tests {
    use syn::parse_str;

    use super::*;

    #[test]
    fn test_parse_operation_id() {
        let input = "operation_id = \"test_operation\"";
        let attr = parse_str::<EndpointAttr>(input).unwrap();
        assert!(attr.operation_id.is_some());
    }

    #[test]
    fn test_parse_request_body() {
        let input = "request_body = Pet";
        let attr = parse_str::<EndpointAttr>(input).unwrap();
        assert!(attr.request_body.is_some());
    }

    #[test]
    fn test_parse_responses() {
        let input = "responses((status_code = 200))";
        let attr = parse_str::<EndpointAttr>(input).unwrap();
        assert_eq!(attr.responses.len(), 1);
    }

    #[test]
    fn test_parse_status_codes() {
        let input = "status_codes(200, 404)";
        let attr = parse_str::<EndpointAttr>(input).unwrap();
        assert_eq!(attr.status_codes.len(), 2);
    }

    #[test]
    #[ignore]
    fn test_parse_parameters() {
        let input = "parameters((\"id\" in path,))";
        let attr = parse_str::<EndpointAttr>(input).unwrap();
        assert_eq!(attr.parameters.len(), 1);
    }

    #[test]
    fn test_parse_tags() {
        let input = "tags(\"pet\", \"store\")";
        let attr = parse_str::<EndpointAttr>(input).unwrap();
        assert_eq!(attr.tags.unwrap().len(), 2);
    }

    #[test]
    fn test_parse_security() {
        let input = "security((\"petstore_auth\" = [\"write:pets\", \"read:pets\"]))";
        let attr = parse_str::<EndpointAttr>(input).unwrap();
        assert!(attr.security.is_some());
    }

    #[test]
    fn test_parse_description() {
        let input = "description = \"test description\"";
        let attr = parse_str::<EndpointAttr>(input).unwrap();
        assert!(attr.description.is_some());
    }

    #[test]
    fn test_parse_summary() {
        let input = "summary = \"test summary\"";
        let attr = parse_str::<EndpointAttr>(input).unwrap();
        assert!(attr.summary.is_some());
    }
}
