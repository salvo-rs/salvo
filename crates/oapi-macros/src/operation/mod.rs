use std::borrow::Cow;
use std::ops::Deref;

use proc_macro2::{Ident, TokenStream as TokenStream2};
use quote::{quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;
use syn::token::Paren;
use syn::{parenthesized, parse::Parse, Token};
use syn::{Expr, ExprPath, Path, Type};

use crate::component::{GenericType, TypeTree};
use crate::endpoint::EndpointAttr;
use crate::schema_type::SchemaType;
use crate::security_requirement::SecurityRequirementAttr;
use crate::Array;

pub(crate) mod example;
pub(crate) mod parameter;
pub(crate) mod request_body;
pub(crate) mod response;
pub(crate) use self::{
    parameter::Parameter,
    request_body::RequestBodyAttr,
    response::{Response, Responses},
};
mod status;

pub struct Operation<'a> {
    operation_id: Option<&'a Expr>,
    summary: Option<&'a String>,
    description: Option<&'a Vec<String>>,
    deprecated: &'a Option<bool>,
    parameters: &'a Vec<Parameter<'a>>,
    request_body: Option<&'a RequestBodyAttr<'a>>,
    responses: &'a Vec<Response<'a>>,
    security: Option<&'a Array<'a, SecurityRequirementAttr>>,
}

impl<'a> Operation<'a> {
    pub(crate) fn new(attr: &'a EndpointAttr) -> Self {
        Self {
            deprecated: &attr.deprecated,
            operation_id: attr.operation_id.as_ref(),
            summary: attr.doc_comments.as_ref().and_then(|comments| comments.iter().next()),
            description: attr.doc_comments.as_ref(),
            parameters: attr.parameters.as_ref(),
            request_body: attr.request_body.as_ref(),
            responses: attr.responses.as_ref(),
            security: attr.security.as_ref(),
        }
    }
    pub(crate) fn modifiers(&self) -> Vec<TokenStream2> {
        let mut modifiers = vec![];
        let oapi = crate::oapi_crate();
        println!("==================================0");
        if let Some(request_body) = &self.request_body {
            println!("==================================1");
            if let Some(content) = &request_body.content {
                match &content {
                    PathType::Ref(path) => {
                        modifiers.push(quote! {
                            {
                                let (path, schema) = <#path as #oapi::oapi::AsSchema>::schema();
                                components.schemas.insert(path, schema);
                            }
                        });
                    }
                    PathType::MediaType(inline) => {
                        let ty = &inline.ty;
                        modifiers.push(quote! {
                            {
                                let (path, schema) = <#ty as #oapi::oapi::AsSchema>::schema();
                                components.schemas.insert(path.into(), schema);
                            }
                        });
                    }
                    _ => {}
                }
            }
        }
        modifiers
    }
}

impl ToTokens for Operation<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let oapi = crate::oapi_crate();
        tokens.extend(quote! { #oapi::oapi::Operation::new() });
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
        if let Some(operation_id) = &self.operation_id {
            tokens.extend(quote_spanned! { operation_id.span() =>
                .operation_id(#operation_id)
            });
        }

        if let Some(deprecated) = self.deprecated {
            tokens.extend(quote!( .deprecated(#deprecated)))
        }

        if let Some(summary) = self.summary {
            tokens.extend(quote! {
                .summary(#summary)
            })
        }

        if let Some(description) = self.description {
            let description = description.join("\n");

            if !description.is_empty() {
                tokens.extend(quote! {
                    .description(#description)
                })
            }
        }

        self.parameters.iter().for_each(|parameter| parameter.to_tokens(tokens));
    }
}

/// Represents either `ref("...")` or `Type` that can be optionally inlined with `inline(Type)`.
#[derive(Debug)]
pub(crate) enum PathType<'p> {
    Ref(Path),
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
            Ok(Self::Ref(ref_stream.parse::<ExprPath>()?.path))
        } else {
            Ok(Self::MediaType(input.parse()?))
        }
    }
}

// inline(syn::Type) | syn::Type
#[derive(Debug)]
pub(crate) struct InlineType<'i> {
    pub(crate) ty: Cow<'i, Type>,
    pub(crate) is_inline: bool,
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
            Some(_) => self.children.as_ref().unwrap().iter().any(|child| child.is_array()),
            None => false,
        }
    }
}
