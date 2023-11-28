use std::borrow::Cow;
use std::ops::Deref;

use proc_macro2::{Ident, TokenStream as TokenStream2};
use quote::quote;
use syn::{parenthesized, parse::Parse, token::Paren, Expr, ExprPath, Path, Token, Type};

use crate::endpoint::EndpointAttr;
use crate::schema_type::SchemaType;
use crate::security_requirement::SecurityRequirementAttr;
use crate::type_tree::{GenericType, TypeTree};
use crate::Array;

pub(crate) mod example;
pub(crate) mod request_body;
pub(crate) use self::request_body::RequestBodyAttr;
use crate::parameter::Parameter;
use crate::response::{Response, ResponseTupleInner};
pub(crate) mod status;

pub(crate) struct Operation<'a> {
    deprecated: &'a Option<bool>,
    operation_id: Option<&'a Expr>,
    tags: &'a Option<Vec<String>>,
    summary: Option<&'a String>,
    description: Option<&'a Vec<String>>,
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
            tags: &attr.tags,
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

        if let Some(rb) = self.request_body {
            modifiers.push(quote! {
                if let Some(request_body) = operation.request_body.as_mut() {
                    request_body.merge(#rb);
                } else {
                    operation.request_body = Some(#rb);
                }
            });
            if let Some(content) = &rb.content {
                modifiers.append(&mut generate_register_schemas(&oapi, content));
            }
        }

        // let responses = Responses(self.responses);
        // modifiers.push(quote! {
        //     .responses(#responses)
        // });
        if let Some(security_requirements) = self.security {
            modifiers.push(quote! {
                operation.securities.append(&mut #security_requirements.into_iter().collect());
            })
        }
        if let Some(operation_id) = &self.operation_id {
            modifiers.push(quote! {
                operation.operation_id = Some(#operation_id);
            });
        }

        if let Some(deprecated) = self.deprecated {
            modifiers.push(quote! {
                operation.deprecated = Some(#deprecated.into());
            })
        }

        if let Some(tags) = self.tags {
            let tags = tags.iter().collect::<Array<_>>();
            modifiers.push(quote! {
                operation.tags.extend(#tags.into_iter().map(|t|t.into()));
            })
        }

        if let Some(summary) = self.summary {
            modifiers.push(quote! {
                operation.summary = Some(#summary.into());
            })
        }

        if let Some(description) = self.description {
            let description = description.join("\n");
            if !description.is_empty() {
                modifiers.push(quote! {
                    operation.description = Some(#description.into());
                })
            }
        }

        self.parameters.iter().for_each(|parameter| {
            modifiers.push(quote! {
                #parameter
            })
        });

        for response in self.responses {
            match response {
                Response::ToResponses(path) => {
                    modifiers.push(quote! {
                        operation.responses.append(&mut <#path as #oapi::oapi::ToResponses>::to_responses(components));
                    });
                }
                Response::Tuple(tuple) => {
                    let code = &tuple.status_code;
                    if let Some(inner) = &tuple.inner {
                        match inner {
                            ResponseTupleInner::Ref(inline) => {
                                let ty = &inline.ty;
                                modifiers.push(quote! {
                                    let _= <#ty as #oapi::oapi::ToSchema>::to_schema(components);
                                });
                            }
                            ResponseTupleInner::Value(value) => {
                                if let Some(content) = &value.response_type {
                                    modifiers.append(&mut generate_register_schemas(&oapi, content));
                                }
                            }
                        }
                    }
                    modifiers.push(quote! {
                        operation.responses.insert(#code, #tuple);
                    });
                }
            }
        }
        modifiers
    }
}

fn generate_register_schemas(oapi: &Ident, content: &PathType) -> Vec<TokenStream2> {
    let mut modifiers = vec![];
    match content {
        PathType::RefPath(path) => {
            modifiers.push(quote! {
                let _ = <#path as #oapi::oapi::ToSchema>::to_schema(components);
            });
        }
        PathType::MediaType(inline) => {
            let ty = &inline.ty;
            modifiers.push(quote! {
                let _ = <#ty as #oapi::oapi::ToSchema>::to_schema(components);
            });
        }
        _ => {}
    }
    modifiers
}

/// Represents either `ref("...")` or `Type` that can be optionally inlined with `inline(Type)`.
#[derive(Debug)]
pub(crate) enum PathType<'p> {
    RefPath(Path),
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
            Ok(Self::RefPath(ref_stream.parse::<ExprPath>()?.path))
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
    pub(crate) fn as_type_tree(&self) -> TypeTree {
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

pub(crate) trait PathTypeTree {
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
            Some(GenericType::Vec | GenericType::Set) => true,
            Some(_) => self.children.as_ref().unwrap().iter().any(|child| child.is_array()),
            None => false,
        }
    }
}
