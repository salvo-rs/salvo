//! This is **private** salvo_oapi codegen library and is not used alone.
//!
//! The library contains macro implementations for salvo_oapi library. Content
//! of the library documentation is available through **salvo_oapi** library itself.
//! Consider browsing via the **salvo_oapi** crate so all links will work correctly.

#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use proc_macro::TokenStream;
use quote::ToTokens;
use syn::parse::{Parse, ParseStream};
use syn::token::Bracket;
use syn::{bracketed, parse_macro_input, DeriveInput, Ident, Item, Token};

mod attribute;
pub(crate) mod bound;
mod component;
mod doc_comment;
mod endpoint;
pub(crate) mod feature;
mod operation;
mod parameter;
pub(crate) mod parse_utils;
mod response;
mod schema;
mod schema_type;
mod security_requirement;
mod shared;
mod type_tree;

pub(crate) use self::{
    component::{ComponentSchema, ComponentSchemaProps},
    endpoint::EndpointAttr,
    feature::Feature,
    operation::Operation,
    parameter::derive::ToParameters,
    parameter::Parameter,
    response::derive::{ToResponse, ToResponses},
    response::Response,
    schema::ToSchema,
    shared::*,
    type_tree::TypeTree,
};
pub(crate) use proc_macro2_diagnostics::{Diagnostic, Level as DiagLevel};
pub(crate) use salvo_serde_util::{self as serde_util, RenameRule, SerdeContainer, SerdeValue};

/// Enhanced of [handler][handler] for generate OpenAPI documention, [Read more][more].
///
/// [handler]: ../salvo_core/attr.handler.html
/// [more]: ../salvo_oapi/endpoint/index.html
#[proc_macro_attribute]
pub fn endpoint(attr: TokenStream, input: TokenStream) -> TokenStream {
    let attr = syn::parse_macro_input!(attr as EndpointAttr);
    let item = parse_macro_input!(input as Item);
    match endpoint::generate(attr, item) {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
/// This is `#[derive]` implementation for [`ToSchema`][to_schema] trait, [Read more][more].
///
/// [to_schema]: ../salvo_oapi/trait.ToSchema.html
/// [more]: ../salvo_oapi/derive.ToSchema.html
#[proc_macro_derive(ToSchema, attributes(salvo))] //attributes(schema)
pub fn derive_to_schema(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        data,
        generics,
        vis,
        ..
    } = syn::parse_macro_input!(input);

    match ToSchema::new(&data, &attrs, &ident, &generics, &vis).and_then(|s| s.try_to_token_stream()) {
        Ok(stream) => stream.into(),
        Err(diag) => diag.emit_as_item_tokens().into(),
    }
}

/// Generate parameters from struct's fields, [Read more][more].
///
/// [more]: ../salvo_oapi/derive.ToParameters.html
#[proc_macro_derive(ToParameters, attributes(salvo))] //attributes(parameter, parameters)
pub fn derive_to_parameters(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = syn::parse_macro_input!(input);

    let stream = ToParameters {
        attrs,
        generics,
        data,
        ident,
    }
    .try_to_token_stream();
    match stream {
        Ok(stream) => stream.into(),
        Err(diag) => diag.emit_as_item_tokens().into(),
    }
}

/// Generate reusable [OpenApi][openapi] response, [Read more][more].
///
/// [openapi]: ../salvo_oapi/struct.OpenApi.html
/// [more]: ../salvo_oapi/derive.ToResponse.html
#[proc_macro_derive(ToResponse, attributes(salvo))] //attributes(response, content, schema))
pub fn derive_to_response(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = syn::parse_macro_input!(input);

    let stream = ToResponse::new(attrs, &data, generics, ident).and_then(|s| s.try_to_token_stream());
    match stream {
        Ok(stream) => stream.into(),
        Err(diag) => diag.emit_as_item_tokens().into(),
    }
}

/// Generate responses with status codes what can be used in [OpenAPI][openapi], [Read more][more].
///
/// [openapi]: ../salvo_oapi/struct.OpenApi.html
/// [more]: ../salvo_oapi/derive.ToResponses.html
#[proc_macro_derive(ToResponses, attributes(salvo))] //attributes(response, schema, ref_response, response))
pub fn to_responses(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = syn::parse_macro_input!(input);

    let stream = ToResponses {
        attributes: attrs,
        ident,
        generics,
        data,
    }
    .try_to_token_stream();

    match stream {
        Ok(stream) => stream.into(),
        Err(diag) => diag.emit_as_item_tokens().into(),
    }
}

#[doc(hidden)]
#[proc_macro]
pub fn schema(input: TokenStream) -> TokenStream {
    struct Schema {
        inline: bool,
        ty: syn::Type,
    }
    impl Parse for Schema {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            let inline = if input.peek(Token![#]) && input.peek2(Bracket) {
                input.parse::<Token![#]>()?;

                let inline;
                bracketed!(inline in input);
                let i = inline.parse::<Ident>()?;
                i == "inline"
            } else {
                false
            };

            let ty = input.parse()?;
            Ok(Self { inline, ty })
        }
    }

    let schema = syn::parse_macro_input!(input as Schema);
    let type_tree = match TypeTree::from_type(&schema.ty) {
        Ok(type_tree) => type_tree,
        Err(diag) => return diag.emit_as_item_tokens().into(),
    };

    let stream = ComponentSchema::new(ComponentSchemaProps {
        features: Some(vec![Feature::Inline(schema.inline.into())]),
        type_tree: &type_tree,
        deprecated: None,
        description: None,
        object_name: "",
    })
    .map(|s| s.to_token_stream());
    match stream {
        Ok(stream) => stream.into(),
        Err(diag) => diag.emit_as_item_tokens().into(),
    }
}

pub(crate) trait IntoInner<T> {
    fn into_inner(self) -> T;
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::parse2;

    use super::*;

    #[test]
    fn test_handler_for_fn() {
        let input = quote! {
            #[endpoint]
            async fn hello() {
                res.render_plain_text("Hello World");
            }
        };
        let item = parse2(input).unwrap();
        assert_eq!(
            endpoint::generate(parse2(quote! {}).unwrap(), item)
                .unwrap()
                .to_string(),
            quote! {
                #[allow(non_camel_case_types)]
                #[derive(Debug)]
                struct hello;
                impl hello {
                    #[endpoint]
                    async fn hello() {
                        {res.render_plain_text("Hello World");}
                    }
                }
                #[salvo::async_trait]
                impl salvo::Handler for hello {
                    async fn handle(
                        &self,
                        __macro_gen_req: &mut salvo::Request,
                        __macro_gen_depot: &mut salvo::Depot,
                        __macro_gen_res: &mut salvo::Response,
                        __macro_gen_ctrl: &mut salvo::FlowCtrl
                    ) {
                        Self::hello().await
                    }
                }
                fn __macro_gen_oapi_endpoint_type_id_hello() -> ::std::any::TypeId {
                    ::std::any::TypeId::of::<hello>()
                }
                fn __macro_gen_oapi_endpoint_creator_hello() -> salvo::oapi::Endpoint {
                    let mut components = salvo::oapi::Components::new();
                    let status_codes: &[salvo::http::StatusCode] = &[];
                    let mut operation = salvo::oapi::Operation::new();
                    if operation.operation_id.is_none() {
                        operation.operation_id = Some(salvo::oapi::naming::assign_name::<hello>(salvo::oapi::naming::NameRule::Auto));
                    }
                    if !status_codes.is_empty() {
                        let responses = std::ops::DerefMut::deref_mut(&mut operation.responses);
                        responses.retain(|k, _| {
                            if let Ok(code) = <salvo::http::StatusCode as std::str::FromStr>::from_str(k) {
                                status_codes.contains(&code)
                            } else {
                                true
                            }
                        });
                    }
                    salvo::oapi::Endpoint {
                        operation,
                        components,
                    }
                }
                salvo::oapi::__private::inventory::submit! {
                    salvo::oapi::EndpointRegistry::save(__macro_gen_oapi_endpoint_type_id_hello, __macro_gen_oapi_endpoint_creator_hello)
                }
            }
            .to_string()
        );
    }
}
