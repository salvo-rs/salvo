use std::borrow::Cow;
use std::ops::Deref;
use std::{io::Error, str::FromStr};

use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use proc_macro_error::abort;
use quote::{format_ident, quote, quote_spanned, ToTokens};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Paren;
use syn::{parenthesized, parse::Parse, Token};
use syn::{Expr, ExprLit, Lit, LitStr, Type};

use crate::component::{GenericType, TypeTree};
use crate::{parse_utils, Deprecated};
use crate::{schema_type::SchemaType, security_requirement::SecurityRequirementAttr, Array};

use self::response::Response;
use self::{parameter::Parameter, request_body::RequestBodyAttr, response::Responses};

pub mod example;
pub mod parameter;
mod request_body;
pub mod response;
mod status;

pub(crate) const PATH_STRUCT_PREFIX: &str = "__path_";

/// PathAttr is parsed `#[salvo_oapi::path(...)]` proc macro and its attributes.
/// Parsed attributes can be used to override or append OpenAPI Path
/// options.
///
/// # Example
/// ```text
/// #[salvo_oapi::path(delete,
///    operation_id = "custom_operation_id",
///    path = "/custom/path/{id}/{digest}",
///    tag = "grouping_tag"
///    request_body = [Foo]
///    responses = [
///         (status = 200, description = "success update Foos", body = [Foo], content_type = "application/json",
///             headers = [
///                 ("foo-bar" = String, description = "custom header value")
///             ]
///         ),
///         (status = 500, description = "internal server error", body = String, content_type = "text/plain",
///             headers = [
///                 ("foo-bar" = String, description = "custom header value")
///             ]
///         ),
///    ],
///    params = [
///      ("id" = u64, description = "Id of Foo"),
///      ("digest", description = "Foos message digest of last updated"),
///      ("x-csrf-token", header, required, deprecated),
///    ]
/// )]
/// ```
#[derive(Default,Debug)]
pub struct PathAttr<'p> {
    path_operation: Option<PathOperation>,
    request_body: Option<RequestBodyAttr<'p>>,
    responses: Vec<Response<'p>>,
    pub(super) path: Option<String>,
    operation_id: Option<Expr>,
    tag: Option<String>,
    params: Vec<Parameter<'p>>,
    security: Option<Array<'p, SecurityRequirementAttr>>,
    context_path: Option<String>,
}

impl Parse for PathAttr<'_> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        const EXPECTED_ATTRIBUTE_MESSAGE: &str = "unexpected identifier, expected any of: operation_id, path, get, post, put, delete, options, head, patch, trace, connect, request_body, responses, params, tag, security, context_path";
        let mut path_attr = PathAttr::default();

        while !input.is_empty() {
            let ident = input.parse::<Ident>().map_err(|error| {
                syn::Error::new(
                    error.span(),
                    format!("{EXPECTED_ATTRIBUTE_MESSAGE}, {error}"),
                )
            })?;
            let attribute_name = &*ident.to_string();

            match attribute_name {
                "operation_id" => {
                    path_attr.operation_id =
                        Some(parse_utils::parse_next(input, || Expr::parse(input))?);
                }
                "path" => {
                    path_attr.path = Some(parse_utils::parse_next_literal_str(input)?);
                }
                "request_body" => {
                    path_attr.request_body = Some(input.parse::<RequestBodyAttr>()?);
                }
                "responses" => {
                    let responses;
                    parenthesized!(responses in input);
                    path_attr.responses =
                        Punctuated::<Response, Token![,]>::parse_terminated(&responses)
                            .map(|punctuated| punctuated.into_iter().collect::<Vec<Response>>())?;
                }
                "params" => {
                    let params;
                    parenthesized!(params in input);
                    path_attr.params =
                        Punctuated::<Parameter, Token![,]>::parse_terminated(&params)
                            .map(|punctuated| punctuated.into_iter().collect::<Vec<Parameter>>())?;
                }
                "tag" => {
                    path_attr.tag = Some(parse_utils::parse_next_literal_str(input)?);
                }
                "security" => {
                    let security;
                    parenthesized!(security in input);
                    path_attr.security = Some(parse_utils::parse_groups(&security)?)
                }
                "context_path" => {
                    path_attr.context_path = Some(parse_utils::parse_next_literal_str(input)?)
                }
                _ => {
                    // any other case it is expected to be path operation
                    if let Some(path_operation) =
                        attribute_name.parse::<PathOperation>().into_iter().next()
                    {
                        path_attr.path_operation = Some(path_operation)
                    } else {
                        return Err(syn::Error::new(ident.span(), EXPECTED_ATTRIBUTE_MESSAGE));
                    }
                }
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(path_attr)
    }
}

/// Path operation type of response
///
/// Instance of path operation can be formed from str parsing with following supported values:
///   * "get"
///   * "post"
///   * "put"
///   * "delete"
///   * "options"
///   * "head"
///   * "patch"
///   * "trace"
#[derive(Debug)]
pub enum PathOperation {
    Get,
    Post,
    Put,
    Delete,
    Options,
    Head,
    Patch,
    Trace,
    Connect,
}

impl FromStr for PathOperation {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "get" => Ok(Self::Get),
            "post" => Ok(Self::Post),
            "put" => Ok(Self::Put),
            "delete" => Ok(Self::Delete),
            "options" => Ok(Self::Options),
            "head" => Ok(Self::Head),
            "patch" => Ok(Self::Patch),
            "trace" => Ok(Self::Trace),
            "connect" => Ok(Self::Connect),
            _ => Err(Error::new(
                std::io::ErrorKind::Other,
                "invalid PathOperation expected one of: get, post, put, delete, options, head, patch, trace, connect",
            )),
        }
    }
}

impl ToTokens for PathOperation {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let root = crate::root_crate();
        let path_item_type = match self {
            Self::Get => quote! { #root::oapi::openapi::PathItemType::Get },
            Self::Post => quote! { #root::oapi::openapi::PathItemType::Post },
            Self::Put => quote! { #root::oapi::openapi::PathItemType::Put },
            Self::Delete => quote! { #root::oapi::openapi::PathItemType::Delete },
            Self::Options => quote! { #root::oapi::openapi::PathItemType::Options },
            Self::Head => quote! { #root::oapi::openapi::PathItemType::Head },
            Self::Patch => quote! { #root::oapi::openapi::PathItemType::Patch },
            Self::Trace => quote! { #root::oapi::openapi::PathItemType::Trace },
            Self::Connect => quote! { #root::oapi::openapi::PathItemType::Connect },
        };

        tokens.extend(path_item_type);
    }
}
pub struct Path<'p> {
    path_attr: PathAttr<'p>,
    fn_name: String,
    path_operation: Option<PathOperation>,
    path: Option<String>,
    doc_comments: Option<Vec<String>>,
    deprecated: Option<bool>,
}

impl<'p> Path<'p> {
    pub fn new(path_attr: PathAttr<'p>, fn_name: &str) -> Self {
        Self {
            path_attr,
            fn_name: fn_name.to_string(),
            path_operation: None,
            path: None,
            doc_comments: None,
            deprecated: None,
        }
    }
    
    pub fn path(mut self, path_provider: impl FnOnce() -> Option<String>) -> Self {
        self.path = path_provider();

        self
    }

    pub fn doc_comments(mut self, doc_comments: Vec<String>) -> Self {
        self.doc_comments = Some(doc_comments);

        self
    }

    pub fn deprecated(mut self, deprecated: Option<bool>) -> Self {
        self.deprecated = deprecated;

        self
    }
}

impl<'p> ToTokens for Path<'p> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let root = crate::root_crate();
        let path_struct = format_ident!("{}{}", PATH_STRUCT_PREFIX, self.fn_name);
        let operation_id = self
            .path_attr
            .operation_id
            .clone()
            .or(Some(ExprLit {
                attrs: vec![],
                lit: Lit::Str(LitStr::new(&self.fn_name, Span::call_site()))
            }.into()))
            .unwrap_or_else(|| {
                abort! {
                    Span::call_site(), "operation id is not defined for path";
                    help = r###"Try to define it in #[salvo_oapi::path(operation_id = {})]"###, &self.fn_name;
                    help = "Did you define the #[salvo_oapi::path(...)] over function?"
                }
            });
        let tag = &*self
            .path_attr
            .tag
            .as_ref()
            .map(ToOwned::to_owned)
            .unwrap_or_default();
        let path_operation = self
            .path_attr
            .path_operation
            .as_ref()
            .or(self.path_operation.as_ref())
            .unwrap_or_else(|| {
                let help = None::<&str>;

                abort! {
                    Span::call_site(), "path operation is not defined for path";
                    help = "Did you forget to define it in #[salvo_oapi::path(get,...)]";
                    help =? help
                }
            });

        let path = self
            .path_attr
            .path
            .as_ref()
            .or(self.path.as_ref())
            .unwrap_or_else(|| {
                let help = None::<&str>;

                abort! {
                    Span::call_site(), "path is not defined for path";
                    help = r###"Did you forget to define it in #[salvo_oapi::path(path = "...")]"###;
                    help =? help
                }
            });

        let path_with_context_path = self
            .path_attr
            .context_path
            .as_ref()
            .map(|context_path| format!("{context_path}{path}"))
            .unwrap_or_else(|| path.to_string());

        let operation: Operation = Operation {
            deprecated: &self.deprecated,
            operation_id,
            summary: self
                .doc_comments
                .as_ref()
                .and_then(|comments| comments.iter().next()),
            description: self.doc_comments.as_ref(),
            parameters: self.path_attr.params.as_ref(),
            request_body: self.path_attr.request_body.as_ref(),
            responses: self.path_attr.responses.as_ref(),
            security: self.path_attr.security.as_ref(),
        };

        tokens.extend(quote! {
            #[allow(non_camel_case_types)]
            #[doc(hidden)]
            pub struct #path_struct;

            impl #root::oapi::Path for #path_struct {
                fn path() -> &'static str {
                    #path_with_context_path
                }

                fn path_item(default_tag: Option<&str>) -> #root::oapi::openapi::path::PathItem {
                    use #root::oapi::openapi::ToArray;
                    use std::iter::FromIterator;
                    #root::oapi::openapi::PathItem::new(
                        #path_operation,
                        #operation.tag(*[Some(#tag), default_tag, Some("crate")].iter()
                            .flatten()
                            .find(|t| !t.is_empty()).unwrap()
                        )
                    )
                }
            }
        });
    }
}

#[derive(Debug)]
struct Operation<'a> {
    operation_id: Expr,
    summary: Option<&'a String>,
    description: Option<&'a Vec<String>>,
    deprecated: &'a Option<bool>,
    parameters: &'a Vec<Parameter<'a>>,
    request_body: Option<&'a RequestBodyAttr<'a>>,
    responses: &'a Vec<Response<'a>>,
    security: Option<&'a Array<'a, SecurityRequirementAttr>>,
}

impl ToTokens for Operation<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let root = crate::root_crate();
        tokens.extend(quote! { #root::oapi::openapi::path::Operation::new() });

        if let Some(request_body) = self.request_body {
            tokens.extend(quote! {
                .request_body(#request_body)
            })
        }

        let responses = Responses(self.responses);
        tokens.extend(quote! {
            .responses(#responses)
        });
        if let Some(security_requirements) = self.security {
            tokens.extend(quote! {
                .securities(#security_requirements)
            })
        }
        let operation_id = &self.operation_id;
        tokens.extend(quote_spanned! { operation_id.span() =>
            .operation_id(#operation_id)
        });

        if let Some(deprecated) = self.deprecated.map(Into::<Deprecated>::into) {
            tokens.extend(quote!( .deprecated(Some(#deprecated))))
        }

        if let Some(summary) = self.summary {
            tokens.extend(quote! {
                .summary(Some(#summary))
            })
        }

        if let Some(description) = self.description {
            let description = description.join("\n");

            if !description.is_empty() {
                tokens.extend(quote! {
                    .description(Some(#description))
                })
            }
        }

        self.parameters
            .iter()
            .for_each(|parameter| parameter.to_tokens(tokens));
    }
}

/// Represents either `ref("...")` or `Type` that can be optionally inlined with `inline(Type)`.
#[derive(Debug)]
enum PathType<'p> {
    Ref(String),
    MediaType(InlineType<'p>),
    InlineSchema(TokenStream2, Type),
}

impl Parse for PathType<'_> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let fork = input.fork();
        let is_ref = if (fork.parse::<Option<Token![ref]>>()?).is_some() {
            fork.peek(Paren)
        } else {
            false
        };

        if is_ref {
            input.parse::<Token![ref]>()?;
            let ref_stream;
            parenthesized!(ref_stream in input);
            Ok(Self::Ref(ref_stream.parse::<LitStr>()?.value()))
        } else {
            Ok(Self::MediaType(input.parse()?))
        }
    }
}

// inline(syn::Type) | syn::Type
#[derive(Debug)]
struct InlineType<'i> {
    ty: Cow<'i, Type>,
    is_inline: bool,
}

impl InlineType<'_> {
    /// Get's the underlying [`syn::Type`] as [`TypeTree`].
    fn as_type_tree(&self) -> TypeTree {
        TypeTree::from_type(&self.ty)
    }
}

impl Parse for InlineType<'_> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let fork = input.fork();
        let is_inline = if let Some(ident) = fork.parse::<Option<Ident>>()? {
            ident == "inline" && fork.peek(Paren)
        } else {
            false
        };

        let ty = if is_inline {
            input.parse::<Ident>()?;
            let inlined;
            parenthesized!(inlined in input);

            inlined.parse::<Type>()?
        } else {
            input.parse::<Type>()?
        };

        Ok(InlineType {
            ty: Cow::Owned(ty),
            is_inline,
        })
    }
}

pub trait PathTypeTree {
    /// Resolve default content type based on current [`Type`].
    fn get_default_content_type(&self) -> &str;

    /// Check whether [`TypeTree`] an option
    fn is_option(&self) -> bool;

    /// Check whether [`TypeTree`] is a Vec, slice, array or other supported array type
    fn is_array(&self) -> bool;
}

impl PathTypeTree for TypeTree<'_> {
    /// Resolve default content type based on current [`Type`].
    fn get_default_content_type(&self) -> &'static str {
        if self.is_array()
            && self
                .children
                .as_ref()
                .map(|children| {
                    children
                        .iter()
                        .flat_map(|child| &child.path)
                        .any(|path| SchemaType(path).is_byte())
                })
                .unwrap_or(false)
        {
            "application/octet-stream"
        } else if self
            .path
            .as_ref()
            .map(|path| SchemaType(path.deref()))
            .map(|schema_type| schema_type.is_primitive())
            .unwrap_or(false)
        {
            "text/plain"
        } else {
            "application/json"
        }
    }

    /// Check whether [`TypeTree`] an option
    fn is_option(&self) -> bool {
        matches!(self.generic_type, Some(GenericType::Option))
    }

    /// Check whether [`TypeTree`] is a Vec, slice, array or other supported array type
    fn is_array(&self) -> bool {
        match self.generic_type {
            Some(GenericType::Vec) => true,
            Some(_) => self
                .children
                .as_ref()
                .unwrap()
                .iter()
                .any(|child| child.is_array()),
            None => false,
        }
    }
}
