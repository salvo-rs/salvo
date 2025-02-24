use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, quote};
use syn::punctuated::Punctuated;
use syn::{Error, Token, parenthesized, parse::Parse, token::Paren};

use crate::component::ComponentSchema;
use crate::feature::attributes::Inline;
use crate::{AnyValue, Array, DiagResult, Required, TryToTokens, parse_utils};

use super::example::Example;
use super::{PathType, PathTypeTree};

/// Parsed information related to request body of path.
///
/// Supported configuration options:
///   * **content** Request body content object type. Can also be array e.g. `content = [String]`.
///   * **content_type** Defines the actual content mime type of a request body such as `application/json`.
///     If not provided really rough guess logic is used. Basically all primitive types are treated as `text/plain`
///     and Object types are expected to be `application/json` by default.
///   * **description** Additional description for request body content type.
/// # Examples
///
/// Request body in path with all supported info. Where content type is treated as a String and expected
/// to be xml.
/// ```text
/// #[salvo_oapi::endpoint(
///    request_body = (content = String, description = "foobar", content_type = "text/xml"),
/// )]
///
/// It is also possible to provide the request body type simply by providing only the content object type.
/// ```text
/// #[salvo_oapi::endpoint(
///    request_body = Foo,
/// )]
/// ```
///
/// Or the request body content can also be an array as well by surrounding it with brackets `[..]`.
/// ```text
/// #[salvo_oapi::endpoint(
///    request_body = [Foo],
/// )]
/// ```
///
/// To define optional request body just wrap the type in `Option<type>`.
/// ```text
/// #[salvo_oapi::endpoint(
///    request_body = Option<[Foo]>,
/// )]
/// ```
#[derive(Default, Debug)]
pub(crate) struct RequestBodyAttr<'r> {
    pub(crate) content: Option<PathType<'r>>,
    pub(crate) content_type: Option<parse_utils::LitStrOrExpr>,
    pub(crate) description: Option<parse_utils::LitStrOrExpr>,
    pub(crate) example: Option<AnyValue>,
    pub(crate) examples: Option<Punctuated<Example, Token![,]>>,
}

impl Parse for RequestBodyAttr<'_> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        const EXPECTED_ATTRIBUTE_MESSAGE: &str =
            "unexpected attribute, expected any of: content, content_type, description, examples";
        let lookahead = input.lookahead1();

        if lookahead.peek(Paren) {
            let group;
            parenthesized!(group in input);

            let mut request_body_attr = RequestBodyAttr::default();
            while !group.is_empty() {
                let ident = group
                    .parse::<Ident>()
                    .map_err(|error| Error::new(error.span(), EXPECTED_ATTRIBUTE_MESSAGE))?;
                let attr_name = &*ident.to_string();

                match attr_name {
                    "content" => {
                        request_body_attr.content = Some(
                            parse_utils::parse_next(&group, || group.parse()).map_err(|error| {
                                Error::new(
                                    error.span(),
                                    format!(
                                        "unexpected token, expected type such as String, {error}",
                                    ),
                                )
                            })?,
                        );
                    }
                    "content_type" => {
                        request_body_attr.content_type =
                            Some(parse_utils::parse_next_lit_str_or_expr(&group)?)
                    }
                    "description" => {
                        request_body_attr.description =
                            Some(parse_utils::parse_next_lit_str_or_expr(&group)?)
                    }
                    "example" => {
                        request_body_attr.example = Some(parse_utils::parse_next(&group, || {
                            AnyValue::parse_json(&group)
                        })?)
                    }
                    "examples" => {
                        request_body_attr.examples =
                            Some(parse_utils::parse_punctuated_within_parenthesis(&group)?)
                    }
                    _ => return Err(Error::new(ident.span(), EXPECTED_ATTRIBUTE_MESSAGE)),
                }

                if !group.is_empty() {
                    group.parse::<Token![,]>()?;
                }
            }

            Ok(request_body_attr)
        } else if lookahead.peek(Token![=]) {
            input.parse::<Token![=]>()?;

            Ok(RequestBodyAttr {
                content: Some(input.parse().map_err(|error| {
                    Error::new(
                        error.span(),
                        format!("unexpected token, expected type such as String, {error}"),
                    )
                })?),
                ..Default::default()
            })
        } else {
            Err(lookahead.error())
        }
    }
}

impl TryToTokens for RequestBodyAttr<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        if let Some(body_type) = &self.content {
            let media_type_schema = match body_type {
                PathType::RefPath(ref_type) => quote! {
                    <#ref_type as #oapi::oapi::schema::Schema>::to_schema(components)
                },
                PathType::MediaType(body_type) => {
                    let type_tree = body_type.as_type_tree()?;
                    ComponentSchema::new(crate::component::ComponentSchemaProps {
                        type_tree: &type_tree,
                        features: Some(vec![Inline::from(body_type.is_inline).into()]),
                        description: None,
                        deprecated: None,
                        object_name: "",
                    })?
                    .to_token_stream()
                }
                PathType::InlineSchema(schema, _) => schema.to_token_stream(),
            };
            let mut content = quote! {
                #oapi::oapi::Content::new(#media_type_schema)
            };

            if let Some(ref example) = self.example {
                content.extend(quote! {
                    .example(#example)
                })
            }
            if let Some(ref examples) = self.examples {
                let examples = examples
                    .iter()
                    .map(|example| {
                        let name = &example.name;
                        quote!((#name, #example))
                    })
                    .collect::<Array<TokenStream>>();
                content.extend(quote!(
                    .extend_examples(#examples)
                ))
            }

            match body_type {
                PathType::RefPath(_) => {
                    tokens.extend(quote! {
                        #oapi::oapi::request_body::RequestBody::new()
                            .add_content("application/json", #content)
                    });
                }
                PathType::MediaType(body_type) => {
                    let type_tree = body_type.as_type_tree()?;
                    let required: Required = (!type_tree.is_option()).into();

                    let content_type = match &self.content_type {
                        Some(content_type) => content_type.to_token_stream(),
                        None => {
                            let content_type = type_tree.get_default_content_type();
                            quote!(#content_type)
                        }
                    };

                    tokens.extend(quote! {
                        #oapi::oapi::request_body::RequestBody::new()
                            .add_content(#content_type, #content)
                            .required(#required)
                    });
                }
                PathType::InlineSchema(_, _) => {
                    unreachable!(
                        "`PathType::InlineSchema` is not implemented for `RequestBodyAttr`"
                    );
                }
            }
        }

        if let Some(description) = &self.description {
            tokens.extend(quote! {
                .description(#description)
            })
        }
        Ok(())
    }
}
