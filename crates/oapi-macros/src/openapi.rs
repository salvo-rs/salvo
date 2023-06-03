use proc_macro2::Ident;
use syn::{
    parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
    token::{And, Comma},
    Error, ExprPath, LitStr, Token, TypePath,
};

use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned, ToTokens};

use crate::{parse_utils, Array, ExternalDocs};

#[derive(Debug)]
struct Schema(TypePath);

impl Parse for Schema {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse().map(Self)
    }
}

#[derive(Debug)]
struct Response(TypePath);

impl Parse for Response {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse().map(Self)
    }
}

#[derive(Default, Debug)]
struct Tag {
    name: String,
    description: Option<String>,
    external_docs: Option<ExternalDocs>,
}

impl Parse for Tag {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        const EXPECTED_ATTRIBUTE: &str = "unexpected token, expected any of: name, description, external_docs";

        let mut tag = Tag::default();

        while !input.is_empty() {
            let ident = input
                .parse::<Ident>()
                .map_err(|error| syn::Error::new(error.span(), format!("{EXPECTED_ATTRIBUTE}, {error}")))?;
            let attribute_name = &*ident.to_string();

            match attribute_name {
                "name" => tag.name = parse_utils::parse_next_literal_str(input)?,
                "description" => tag.description = Some(parse_utils::parse_next_literal_str(input)?),
                "external_docs" => {
                    let content;
                    parenthesized!(content in input);
                    tag.external_docs = Some(content.parse::<ExternalDocs>()?);
                }
                _ => return Err(syn::Error::new(ident.span(), EXPECTED_ATTRIBUTE)),
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(tag)
    }
}

impl ToTokens for Tag {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        let name = &self.name;
        tokens.extend(quote! {
            #oapi::oapi::tag::Tag::default().name(#name)
        });

        if let Some(ref description) = self.description {
            tokens.extend(quote! {
                .description(#description)
            });
        }

        if let Some(ref external_docs) = self.external_docs {
            tokens.extend(quote! {
                .external_docs(#external_docs)
            });
        }
    }
}

// (url = "http:://url", description = "description", variables(...))
#[derive(Default, Debug)]
struct Server {
    url: String,
    description: Option<String>,
    variables: Punctuated<ServerVariable, Comma>,
}

impl Parse for Server {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let server_stream;
        parenthesized!(server_stream in input);
        let mut server = Server::default();
        while !server_stream.is_empty() {
            let ident = server_stream.parse::<Ident>()?;
            let attribute_name = &*ident.to_string();

            match attribute_name {
                "url" => {
                    server.url = parse_utils::parse_next(&server_stream, || server_stream.parse::<LitStr>())?.value()
                }
                "description" => {
                    server.description =
                        Some(parse_utils::parse_next(&server_stream, || server_stream.parse::<LitStr>())?.value())
                }
                "variables" => server.variables = parse_utils::parse_punctuated_within_parenthesis(&server_stream)?,
                _ => {
                    return Err(Error::new(
                        ident.span(),
                        format!("unexpected attribute: {attribute_name}, expected one of: url, description, variables"),
                    ))
                }
            }

            if !server_stream.is_empty() {
                server_stream.parse::<Comma>()?;
            }
        }

        Ok(server)
    }
}

impl ToTokens for Server {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        let url = &self.url;
        let description = &self
            .description
            .as_ref()
            .map(|description| quote! { .description(#description) });

        let parameters = self
            .variables
            .iter()
            .map(|variable| {
                let name = &variable.name;
                let default_value = &variable.default;
                let description = &variable
                    .description
                    .as_ref()
                    .map(|description| quote! { .description(#description) });
                let enum_values = &variable.enum_values.as_ref().map(|enum_values| {
                    let enum_values = enum_values.iter().collect::<Array<&LitStr>>();

                    quote! { .enum_values(#enum_values) }
                });

                quote! {
                    .add_variable(#name, #oapi::oapi::server::ServerVariable::new()
                        .default_value(#default_value)
                        #description
                        #enum_values
                    )
                }
            })
            .collect::<TokenStream>();

        tokens.extend(quote! {
            #oapi::oapi::server::ServerBuilder::new()
                .url(#url)
                #description
                #parameters
        })
    }
}

// ("username" = (default = "demo", description = "This is default username for the API")),
// ("port" = (enum_values = (8080, 5000, 4545)))
#[derive(Default, Debug)]
struct ServerVariable {
    name: String,
    default_value: String,
    description: Option<String>,
    enum_values: Option<Punctuated<LitStr, Comma>>,
}

impl Parse for ServerVariable {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let variable_stream;
        parenthesized!(variable_stream in input);
        let mut server_variable = ServerVariable {
            name: variable_stream.parse::<LitStr>()?.value(),
            ..ServerVariable::default()
        };

        variable_stream.parse::<Token![=]>()?;
        let content;
        parenthesized!(content in variable_stream);

        while !content.is_empty() {
            let ident = content.parse::<Ident>()?;
            let attribute_name = &*ident.to_string();

            match attribute_name {
                "default" => {
                    server_variable.default_value =
                        parse_utils::parse_next(&content, || content.parse::<LitStr>())?.value()
                }
                "description" => {
                    server_variable.description =
                        Some(parse_utils::parse_next(&content, || content.parse::<LitStr>())?.value())
                }
                "enum_values" => {
                    server_variable.enum_values = Some(parse_utils::parse_punctuated_within_parenthesis(&content)?)
                }
                _ => {
                    return Err(Error::new(
                        ident.span(),
                        format!(
                        "unexpected attribute: {attribute_name}, expected one of: default, description, enum_values"
                    ),
                    ))
                }
            }

            if !content.is_empty() {
                content.parse::<Comma>()?;
            }
        }

        Ok(server_variable)
    }
}

#[derive(Default, Debug)]
struct Components {
    schemas: Vec<Schema>,
    responses: Vec<Response>,
}

impl Parse for Components {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        parenthesized!(content in input);
        const EXPECTED_ATTRIBUTE: &str = "unexpected attribute. expected one of: schemas, responses";

        let mut schemas: Vec<Schema> = Vec::new();
        let mut responses: Vec<Response> = Vec::new();

        while !content.is_empty() {
            let ident = content
                .parse::<Ident>()
                .map_err(|error| Error::new(error.span(), format!("{EXPECTED_ATTRIBUTE}, {error}")))?;
            let attribute = &*ident.to_string();

            match attribute {
                "schemas" => schemas.append(
                    &mut parse_utils::parse_punctuated_within_parenthesis(&content)?
                        .into_iter()
                        .collect(),
                ),
                "responses" => responses.append(
                    &mut parse_utils::parse_punctuated_within_parenthesis(&content)?
                        .into_iter()
                        .collect(),
                ),
                _ => return Err(syn::Error::new(ident.span(), EXPECTED_ATTRIBUTE)),
            }

            if !content.is_empty() {
                content.parse::<Token![,]>()?;
            }
        }

        Ok(Self { schemas, responses })
    }
}

impl ToTokens for Components {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        if self.schemas.is_empty() && self.responses.is_empty() {
            return;
        }

        let builder_tokens =
            self.schemas
                .iter()
                .fold(quote! { #oapi::oapi::Components::new() }, |mut tokens, schema| {
                    let Schema(path) = schema;

                    tokens.extend(quote_spanned!(path.span()=>
                         .schema_from::<#path>()
                    ));

                    tokens
                });

        let builder_tokens = self
            .responses
            .iter()
            .fold(builder_tokens, |mut builder_tokens, responses| {
                let Response(path) = responses;

                builder_tokens.extend(quote_spanned! {path.span() =>
                    .response_from::<#path>()
                });
                builder_tokens
            });

        tokens.extend(quote! { #builder_tokens });
    }
}
