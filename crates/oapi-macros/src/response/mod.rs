mod derive;
mod link;
mod parse;

use std::borrow::Cow;

use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::{Attribute, DeriveInput, Error, ExprPath, LitInt, LitStr, Token, parenthesized};

use crate::component::ComponentSchema;
use crate::feature::attributes::Inline;
use crate::operation::{
    InlineType, PathType, PathTypeTree, example::Example, status::STATUS_CODES,
};
use crate::type_tree::TypeTree;
use crate::{AnyValue, Array, DiagResult, Diagnostic, TryToTokens, attribute, parse_utils};

use self::derive::{ToResponse, ToResponses};
use self::link::LinkTuple;

pub(crate) fn to_response(input: DeriveInput) -> DiagResult<TokenStream> {
    let DeriveInput {
        attrs,
        ident,
        data,
        generics,
        ..
    } = input;
    ToResponse::new(&attrs, &data, &ident, &generics).and_then(|s| s.try_to_token_stream())
}

pub(crate) fn to_responses(input: DeriveInput) -> DiagResult<TokenStream> {
    let DeriveInput {
        attrs,
        ident,
        data,
        generics,
        ..
    } = input;
    ToResponses {
        attributes: &attrs,
        ident: &ident,
        generics: &generics,
        data: &data,
    }
    .try_to_token_stream()
}

#[derive(Debug)]
pub(crate) enum Response<'r> {
    /// A type that implements `salvo_oapi::ToResponses`.
    ToResponses(ExprPath),
    /// The tuple definition of a response.
    Tuple(ResponseTuple<'r>),
}

impl Parse for Response<'_> {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.fork().parse::<ExprPath>().is_ok() {
            Ok(Self::ToResponses(input.parse()?))
        } else {
            let response;
            parenthesized!(response in input);
            Ok(Self::Tuple(response.parse()?))
        }
    }
}

/// Parsed representation of response attributes from `#[salvo_oapi::endpoint]` attribute.
#[derive(Default, Debug)]
pub(crate) struct ResponseTuple<'r> {
    pub(crate) status_code: ResponseStatusCode,
    pub(crate) inner: Option<ResponseTupleInner<'r>>,
}

const RESPONSE_INCOMPATIBLE_ATTRIBUTES_MSG: &str =
    "The `response` attribute may only be used in conjunction with the `status_code` attribute";

impl<'r> ResponseTuple<'r> {
    // This will error if the `response` attribute has already been set
    fn as_value(&mut self, span: Span) -> syn::Result<&mut ResponseValue<'r>> {
        if self.inner.is_none() {
            self.inner = Some(ResponseTupleInner::Value(ResponseValue::default()));
        }
        if let ResponseTupleInner::Value(val) = self
            .inner
            .as_mut()
            .expect("inner value shoule not be `None`")
        {
            Ok(val)
        } else {
            Err(Error::new(span, RESPONSE_INCOMPATIBLE_ATTRIBUTES_MSG))
        }
    }

    // Use with the `response` attribute, this will fail if an incompatible attribute has already been set
    fn set_ref_type(&mut self, span: Span, ty: InlineType<'r>) -> syn::Result<()> {
        match &mut self.inner {
            None => self.inner = Some(ResponseTupleInner::Ref(ty)),
            Some(ResponseTupleInner::Ref(r)) => *r = ty,
            Some(ResponseTupleInner::Value(_)) => {
                return Err(Error::new(span, RESPONSE_INCOMPATIBLE_ATTRIBUTES_MSG));
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum ResponseTupleInner<'r> {
    Value(ResponseValue<'r>),
    Ref(InlineType<'r>),
}

impl Parse for ResponseTuple<'_> {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        const EXPECTED_ATTRIBUTE_MESSAGE: &str = "unexpected attribute, expected any of: status_code, description, body, content_type, headers, example, examples, response";

        let mut response = ResponseTuple::default();

        while !input.is_empty() {
            let ident = input.parse::<Ident>().map_err(|error| {
                Error::new(
                    error.span(),
                    format!("{EXPECTED_ATTRIBUTE_MESSAGE}, {error}"),
                )
            })?;
            let attr_name = &*ident.to_string();

            match attr_name {
                "status_code" => {
                    response.status_code =
                        parse_utils::parse_next(input, || input.parse::<ResponseStatusCode>())?;
                }
                "description" => {
                    response.as_value(input.span())?.description = parse::description(input)?;
                }
                "body" => {
                    response.as_value(input.span())?.response_type =
                        Some(parse_utils::parse_next(input, || input.parse())?);
                }
                "content_type" => {
                    response.as_value(input.span())?.content_type =
                        Some(parse::content_type(input)?);
                }
                "headers" => {
                    response.as_value(input.span())?.headers = parse::headers(input)?;
                }
                "example" => {
                    response.as_value(input.span())?.example = Some(parse::example(input)?);
                }
                "examples" => {
                    response.as_value(input.span())?.examples = Some(parse::examples(input)?);
                }
                "content" => {
                    response.as_value(input.span())?.contents =
                        parse_utils::parse_punctuated_within_parenthesis(input)?;
                }
                "links" => {
                    response.as_value(input.span())?.links =
                        parse_utils::parse_punctuated_within_parenthesis(input)?;
                }
                "response" => {
                    response.set_ref_type(
                        input.span(),
                        parse_utils::parse_next(input, || input.parse())?,
                    )?;
                }
                _ => return Err(Error::new(ident.span(), EXPECTED_ATTRIBUTE_MESSAGE)),
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        if response.inner.is_none() {
            response.inner = Some(ResponseTupleInner::Value(ResponseValue::default()))
        }

        Ok(response)
    }
}

impl<'r> From<ResponseValue<'r>> for ResponseTuple<'r> {
    fn from(value: ResponseValue<'r>) -> Self {
        ResponseTuple {
            inner: Some(ResponseTupleInner::Value(value)),
            ..Default::default()
        }
    }
}

impl<'r> From<(ResponseStatusCode, ResponseValue<'r>)> for ResponseTuple<'r> {
    fn from((status_code, response_value): (ResponseStatusCode, ResponseValue<'r>)) -> Self {
        ResponseTuple {
            inner: Some(ResponseTupleInner::Value(response_value)),
            status_code,
        }
    }
}

pub(crate) struct DeriveResponsesAttributes<T> {
    derive_value: T,
    description: parse_utils::LitStrOrExpr,
}

impl From<DeriveResponsesAttributes<DeriveToResponsesValue>> for ResponseValue<'_> {
    fn from(value: DeriveResponsesAttributes<DeriveToResponsesValue>) -> Self {
        Self::from_derive_to_responses_value(value.derive_value, value.description)
    }
}

impl From<DeriveResponsesAttributes<Option<DeriveToResponseValue>>> for ResponseValue<'_> {
    fn from(
        DeriveResponsesAttributes::<Option<DeriveToResponseValue>> {
            derive_value,
            description,
        }: DeriveResponsesAttributes<Option<DeriveToResponseValue>>,
    ) -> Self {
        if let Some(derive_value) = derive_value {
            ResponseValue::from_derive_to_response_value(derive_value, description)
        } else {
            ResponseValue {
                description,
                ..Default::default()
            }
        }
    }
}

#[derive(Default, Debug)]
pub(crate) struct ResponseValue<'r> {
    pub(crate) description: parse_utils::LitStrOrExpr,
    pub(crate) response_type: Option<PathType<'r>>,
    pub(crate) content_type: Option<Vec<parse_utils::LitStrOrExpr>>,
    headers: Vec<Header>,
    pub(crate) example: Option<AnyValue>,
    pub(crate) examples: Option<Punctuated<Example, Token![,]>>,
    contents: Punctuated<Content<'r>, Token![,]>,
    links: Punctuated<LinkTuple, Comma>,
}

impl<'r> ResponseValue<'r> {
    fn from_derive_to_response_value(
        derive_value: DeriveToResponseValue,
        description: parse_utils::LitStrOrExpr,
    ) -> Self {
        Self {
            description: if derive_value.description.is_empty() && !description.is_empty() {
                description
            } else {
                derive_value.description
            },
            headers: derive_value.headers,
            example: derive_value.example.map(|(example, _)| example),
            examples: derive_value.examples.map(|(examples, _)| examples),
            content_type: derive_value.content_type,
            ..Default::default()
        }
    }

    fn from_derive_to_responses_value(
        response_value: DeriveToResponsesValue,
        description: parse_utils::LitStrOrExpr,
    ) -> Self {
        ResponseValue {
            description: if response_value.description.is_empty() && !description.is_empty() {
                description
            } else {
                response_value.description
            },
            headers: response_value.headers,
            example: response_value.example.map(|(example, _)| example),
            examples: response_value.examples.map(|(examples, _)| examples),
            content_type: response_value.content_type,
            ..Default::default()
        }
    }

    fn response_type(mut self, response_type: PathType<'r>) -> Self {
        self.response_type = Some(response_type);
        self
    }
}

impl TryToTokens for ResponseTuple<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        match self
            .inner
            .as_ref()
            .expect("inner value should not be `None`")
        {
            ResponseTupleInner::Ref(res) => {
                let path = &res.ty;
                tokens.extend(quote_spanned! {path.span()=>
                    <#path as #oapi::oapi::ToResponse>::to_response(components)
                });
            }
            ResponseTupleInner::Value(val) => {
                let description = &val.description;
                tokens.extend(quote! {
                    #oapi::oapi::Response::new(#description)
                });

                let create_content = |path_type: &PathType,
                                      example: &Option<AnyValue>,
                                      examples: &Option<Punctuated<Example, Token![,]>>|
                 -> DiagResult<TokenStream> {
                    let content_schema = match path_type {
                        PathType::RefPath(ref_type) => quote! {
                            <#ref_type as #oapi::oapi::ToSchema>::to_schema(components)
                        }
                        .to_token_stream(),
                        PathType::MediaType(path_type) => {
                            let type_tree = path_type.as_type_tree()?;

                            ComponentSchema::new(crate::component::ComponentSchemaProps {
                                type_tree: &type_tree,
                                features: Some(vec![Inline::from(path_type.is_inline).into()]),
                                description: None,
                                deprecated: None,
                                object_name: "",
                            })?
                            .to_token_stream()
                        }
                        PathType::InlineSchema(schema, _) => schema.to_token_stream(),
                    };

                    let mut content = quote! { #oapi::oapi::Content::new(#content_schema) };

                    if let Some(example) = &example {
                        content.extend(quote! {
                            .example(#example)
                        })
                    }
                    if let Some(examples) = &examples {
                        let examples = examples
                            .iter()
                            .map(|example| {
                                let name = &example.name;
                                quote!((#name, #example))
                            })
                            .collect::<Array<TokenStream>>();
                        content.extend(quote!( .extend_examples(#examples)))
                    }
                    Ok(quote! {
                        #content
                    })
                };

                if let Some(response_type) = &val.response_type {
                    let content = create_content(response_type, &val.example, &val.examples)?;

                    if let Some(content_types) = val.content_type.as_ref() {
                        content_types.iter().for_each(|content_type| {
                            tokens.extend(quote! {
                                .add_content(#content_type, #content)
                            })
                        })
                    } else {
                        match response_type {
                            PathType::RefPath(_) => {
                                tokens.extend(quote! {
                                    .add_content("application/json", #content)
                                });
                            }
                            PathType::MediaType(path_type) => {
                                let type_tree = path_type.as_type_tree()?;
                                let default_type = type_tree.get_default_content_type();
                                tokens.extend(quote! {
                                    .add_content(#default_type, #content)
                                })
                            }
                            PathType::InlineSchema(_, ty) => {
                                let type_tree = TypeTree::from_type(ty)?;
                                let default_type = type_tree.get_default_content_type();
                                tokens.extend(quote! {
                                    .add_content(#default_type, #content)
                                })
                            }
                        }
                    }
                }

                val.contents
                    .iter()
                    .map(|Content(content_type, body, example, examples)| {
                        let content = create_content(body, example, examples);
                        (Cow::Borrowed(&**content_type), content)
                    })
                    .for_each(|(content_type, content)| {
                        let content = match content {
                            Ok(tokens) => tokens,
                            Err(diag) => diag.emit_as_expr_tokens(),
                        };
                        tokens.extend(quote! { .add_content(#content_type, #content) })
                    });

                val.headers.iter().for_each(|header| {
                    let name = &header.name;
                    let header = match header.try_to_token_stream() {
                        Ok(tokens) => tokens,
                        Err(diag) => diag.emit_as_expr_tokens(),
                    };
                    tokens.extend(quote! {
                        .add_header(#name, #header)
                    })
                });

                val.links.iter().for_each(|LinkTuple(name, link)| {
                    tokens.extend(quote! {
                        .add_link(#name, #link)
                    })
                });
            }
        }
        Ok(())
    }
}

trait DeriveResponseValue: Parse {
    fn merge_from(self, other: Self) -> Self;

    fn from_attributes(attrs: &[Attribute]) -> DiagResult<Option<Self>> {
        Ok(attrs
            .iter()
            .filter_map(|attr| {
                if attr.path().is_ident("salvo") {
                    attribute::find_nested_list(attr, "response")
                        .ok()
                        .flatten()
                        .map(|metas| metas.parse_args::<Self>().map_err(Diagnostic::from))
                } else {
                    None
                }
            })
            .collect::<Result<Vec<_>, Diagnostic>>()?
            .into_iter()
            .reduce(|acc, item| acc.merge_from(item)))
    }
}

#[derive(Default, Debug)]
struct DeriveToResponseValue {
    content_type: Option<Vec<parse_utils::LitStrOrExpr>>,
    headers: Vec<Header>,
    description: parse_utils::LitStrOrExpr,
    example: Option<(AnyValue, Ident)>,
    examples: Option<(Punctuated<Example, Token![,]>, Ident)>,
}

impl DeriveResponseValue for DeriveToResponseValue {
    fn merge_from(mut self, other: Self) -> Self {
        if other.content_type.is_some() {
            self.content_type = other.content_type;
        }
        if !other.headers.is_empty() {
            self.headers = other.headers;
        }
        if !other.description.is_empty() {
            self.description = other.description;
        }
        if other.example.is_some() {
            self.example = other.example;
        }
        if other.examples.is_some() {
            self.examples = other.examples;
        }

        self
    }
}

impl Parse for DeriveToResponseValue {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut response = DeriveToResponseValue::default();

        while !input.is_empty() {
            let ident = input.parse::<Ident>()?;
            let attr_name = &*ident.to_string();

            match attr_name {
                "description" => {
                    response.description = parse::description(input)?;
                }
                "content_type" => {
                    response.content_type = Some(parse::content_type(input)?);
                }
                "headers" => {
                    response.headers = parse::headers(input)?;
                }
                "example" => {
                    response.example = Some((parse::example(input)?, ident));
                }
                "examples" => {
                    response.examples = Some((parse::examples(input)?, ident));
                }
                _ => {
                    return Err(Error::new(
                        ident.span(),
                        format!(
                            "unexpected attribute: {attr_name}, expected any of: inline, description, content_type, headers, example"
                        ),
                    ));
                }
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(response)
    }
}

#[derive(Default)]
struct DeriveToResponsesValue {
    status_code: ResponseStatusCode,
    content_type: Option<Vec<parse_utils::LitStrOrExpr>>,
    headers: Vec<Header>,
    description: parse_utils::LitStrOrExpr,
    example: Option<(AnyValue, Ident)>,
    examples: Option<(Punctuated<Example, Token![,]>, Ident)>,
}

impl DeriveResponseValue for DeriveToResponsesValue {
    fn merge_from(mut self, other: Self) -> Self {
        self.status_code = other.status_code;

        if other.content_type.is_some() {
            self.content_type = other.content_type;
        }
        if !other.headers.is_empty() {
            self.headers = other.headers;
        }
        if !other.description.is_empty() {
            self.description = other.description;
        }
        if other.example.is_some() {
            self.example = other.example;
        }
        if other.examples.is_some() {
            self.examples = other.examples;
        }

        self
    }
}

impl Parse for DeriveToResponsesValue {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut response = DeriveToResponsesValue::default();
        const MISSING_STATUS_ERROR: &str = "missing expected `status_code` attribute";
        let first_span = input.span();

        let status_ident = input
            .parse::<Ident>()
            .map_err(|error| Error::new(error.span(), MISSING_STATUS_ERROR))?;

        if status_ident == "status_code" {
            response.status_code =
                parse_utils::parse_next(input, || input.parse::<ResponseStatusCode>())?;
        } else {
            return Err(Error::new(status_ident.span(), MISSING_STATUS_ERROR));
        }

        if response.status_code.to_token_stream().is_empty() {
            return Err(Error::new(first_span, MISSING_STATUS_ERROR));
        }

        if !input.is_empty() {
            input.parse::<Token![,]>()?;
        }

        while !input.is_empty() {
            let ident = input.parse::<Ident>()?;
            let attr_name = &*ident.to_string();

            match attr_name {
                "description" => {
                    response.description = parse::description(input)?;
                }
                "content_type" => {
                    response.content_type = Some(parse::content_type(input)?);
                }
                "headers" => {
                    response.headers = parse::headers(input)?;
                }
                "example" => {
                    response.example = Some((parse::example(input)?, ident));
                }
                "examples" => {
                    response.examples = Some((parse::examples(input)?, ident));
                }
                _ => {
                    return Err(Error::new(
                        ident.span(),
                        format!(
                            "unexpected attribute: {attr_name}, expected any of: description, content_type, headers, example, examples"
                        ),
                    ));
                }
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(response)
    }
}

#[derive(Default, Debug)]
pub(crate) struct ResponseStatusCode(TokenStream);

impl Parse for ResponseStatusCode {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        fn parse_lit_int(input: ParseStream) -> syn::Result<Cow<'_, str>> {
            input.parse::<LitInt>()?.base10_parse().map(Cow::Owned)
        }

        fn parse_lit_str_status_range(input: ParseStream) -> syn::Result<Cow<'_, str>> {
            const VALID_STATUS_RANGES: [&str; 6] = ["default", "1XX", "2XX", "3XX", "4XX", "5XX"];

            input
                .parse::<LitStr>()
                .and_then(|lit_str| {
                    let value = lit_str.value();
                    if !VALID_STATUS_RANGES.contains(&value.as_str()) {
                        Err(Error::new(
                            value.span(),
                            format!(
                                "Invalid status code range, expected one of: {}",
                                VALID_STATUS_RANGES.join(", "),
                            ),
                        ))
                    } else {
                        Ok(value)
                    }
                })
                .map(Cow::Owned)
        }

        fn parse_http_status_code(input: ParseStream) -> syn::Result<TokenStream> {
            let http_status_path = input.parse::<ExprPath>()?;
            let last_segment = http_status_path
                .path
                .segments
                .last()
                .expect("Expected at least one segment in http StatusCode");

            STATUS_CODES
                .iter()
                .find_map(|(code, name)| {
                    if last_segment.ident == name {
                        Some(code.to_string().to_token_stream())
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    Error::new(
                        last_segment.span(),
                        format!(
                            "No associate item `{}` found for struct `http::StatusCode`",
                            last_segment.ident
                        ),
                    )
                })
        }

        let lookahead = input.lookahead1();
        if lookahead.peek(LitInt) {
            parse_lit_int(input).map(|status_code| Self(status_code.to_token_stream()))
        } else if lookahead.peek(LitStr) {
            parse_lit_str_status_range(input).map(|status_code| Self(status_code.to_token_stream()))
        } else if lookahead.peek(syn::Ident) {
            parse_http_status_code(input).map(Self)
        } else {
            Err(lookahead.error())
        }
    }
}

impl ToTokens for ResponseStatusCode {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
    }
}

// content(
//   ("application/json" = Response, example = "...", examples(..., ...)),
//   ("application/json2" = Response2, example = "...", examples("...", "..."))
// )
#[derive(Debug)]
struct Content<'c>(
    String,
    PathType<'c>,
    Option<AnyValue>,
    Option<Punctuated<Example, Token![,]>>,
);

impl Parse for Content<'_> {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        parenthesized!(content in input);

        let content_type = content.parse::<LitStr>()?;
        content.parse::<Token![=]>()?;
        let body = content.parse()?;
        content.parse::<Option<Token![,]>>()?;
        let mut example = None::<AnyValue>;
        let mut examples = None::<Punctuated<Example, Token![,]>>;

        while !content.is_empty() {
            let ident = content.parse::<Ident>()?;
            let attr_name = &*ident.to_string();
            match attr_name {
                "example" => {
                    example = Some(parse_utils::parse_next(&content, || {
                        AnyValue::parse_json(&content)
                    })?)
                }
                "examples" => {
                    examples = Some(parse_utils::parse_punctuated_within_parenthesis(&content)?)
                }
                _ => {
                    return Err(Error::new(
                        ident.span(),
                        format!(
                            "unexpected attribute: {ident}, expected one of: example, examples"
                        ),
                    ));
                }
            }

            if !content.is_empty() {
                content.parse::<Token![,]>()?;
            }
        }

        Ok(Content(content_type.value(), body, example, examples))
    }
}

// pub(crate) struct Responses<'a>(pub(crate) &'a [Response<'a>]);

// impl TryToTokens for Responses<'_> {
//     fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
//         let oapi = crate::oapi_crate();
//         tokens.extend(
//             self.0
//                 .iter()
//                 .fold(quote! { #oapi::oapi::Responses::new() }, |mut acc, response| {
//                     match response {
//                         Response::ToResponses(path) => {
//                             let span = path.span();
//                             acc.extend(quote_spanned! {span =>
//                                 .append(&mut <#path as #oapi::oapi::ToResponses>::to_responses(components))
//                             })
//                         }
//                         Response::Tuple(response) => {
//                             let code = &response.status_code;
//                             acc.extend(quote! { .response(#code, #response) });
//                         }
//                     }

//                     acc
//                 }),
//         );
//     }
// }

/// Parsed representation of response header defined in `#[salvo_oapi::endpoint(..)]` attribute.
///
/// Supported configuration format is `("x-my-header-name" = type, description = "optional description of header")`.
/// The `= type` and the `description = ".."` are optional configurations thus so the same configuration
/// could be written as follows: `("x-my-header-name")`.
///
/// The `type` can be any typical type supported as a header argument such as `String, i32, u64, bool` etc.
/// and if not provided it will default to `String`.
///
/// # Examples
///
/// Example of 200 success response which does return nothing back in response body, but returns a
/// new csrf token in response headers.
/// ```text
/// #[salvo_oapi::endpoint(
///     ...
///     responses = [
///         (status_code = 200, description = "success response",
///             headers = [
///                 ("xrfs-token" = String, description = "New csrf token sent back in response header")
///             ]
///         ),
///     ]
/// )]
/// ```
///
/// Example with default values.
/// ```text
/// #[salvo_oapi::endpoint(
///     ...
///     responses = [
///         (status_code = 200, description = "success response",
///             headers = [
///                 ("xrfs-token")
///             ]
///         ),
///     ]
/// )]
/// ```
///
/// Example with multiple headers with default values.
/// ```text
/// #[salvo_oapi::endpoint(
///     ...
///     responses = [
///         (status_code = 200, description = "success response",
///             headers = [
///                 ("xrfs-token"),
///                 ("another-header"),
///             ]
///         ),
///     ]
/// )]
/// ```
#[derive(Default, Debug)]
struct Header {
    name: String,
    value_type: Option<InlineType<'static>>,
    description: Option<String>,
}

impl Parse for Header {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut header = Header {
            name: input.parse::<LitStr>()?.value(),
            ..Default::default()
        };

        if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;

            header.value_type = Some(input.parse().map_err(|error| {
                Error::new(
                    error.span(),
                    format!("unexpected token, expected type such as String, {error}"),
                )
            })?);
        }

        if !input.is_empty() {
            input.parse::<Token![,]>()?;
        }

        if input.peek(syn::Ident) {
            input
                .parse::<Ident>()
                .map_err(|error| {
                    Error::new(
                        error.span(),
                        format!("unexpected attribute, expected: description, {error}"),
                    )
                })
                .and_then(|ident| {
                    if ident != "description" {
                        return Err(Error::new(
                            ident.span(),
                            "unexpected attribute, expected: description",
                        ));
                    }
                    Ok(ident)
                })?;
            input.parse::<Token![=]>()?;
            header.description = Some(input.parse::<LitStr>()?.value());
        }

        Ok(header)
    }
}

impl TryToTokens for Header {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        if let Some(header_type) = &self.value_type {
            // header property with custom type
            let type_tree = header_type.as_type_tree()?;

            let media_type_schema = ComponentSchema::new(crate::component::ComponentSchemaProps {
                type_tree: &type_tree,
                features: Some(vec![Inline::from(header_type.is_inline).into()]),
                description: None,
                deprecated: None,
                object_name: "",
            })?
            .to_token_stream();

            tokens.extend(quote! {
                #oapi::oapi::Header::new(#media_type_schema)
            })
        } else {
            // default header (string type)
            tokens.extend(quote! {
                Into::<#oapi::oapi::Header>::into(#oapi::oapi::Header::default())
            })
        };

        if let Some(description) = &self.description {
            tokens.extend(quote! {
                .description(#description)
            })
        }
        Ok(())
    }
}
