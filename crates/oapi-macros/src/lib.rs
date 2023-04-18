//! This is **private** salvo_oapi codegen library and is not used alone.
//!
//! The library contains macro implementations for salvo_oapi library. Content
//! of the library documentation is available through **salvo_oapi** library itself.
//! Consider browsing via the **salvo_oapi** crate so all links will work correctly.

#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::ops::Deref;

use component::schema::Schema;
use doc_comment::CommentAttributes;

use component::into_params::IntoParams;
use proc_macro::TokenStream;
use proc_macro_error::{abort, proc_macro_error};
use quote::{quote, ToTokens, TokenStreamExt};

use proc_macro2::{Group, Ident, Punct, Span, TokenStream as TokenStream2};
use syn::{
    bracketed,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    token::Bracket,
    DeriveInput, ExprPath, ItemFn, Lit, LitStr, Member, Token,
};
use proc_macro_crate::{crate_name, FoundCrate};

mod component;
mod doc_comment;
mod openapi;
mod path;
mod schema_type;
mod security_requirement;
mod parse_utils;
mod endpoint;

use crate::path::{Path, PathAttr};

use self::{
    component::{
        features::{self, Feature},
        ComponentSchema, ComponentSchemaProps, TypeTree,
    },
    path::response::derive::{IntoResponses, ToResponse},
};

// https://github.com/bkchr/proc-macro-crate/issues/14
pub(crate) fn root_crate() -> syn::Ident {
    match crate_name("salvo-oapi") {
        Ok(oapi) => match oapi {
            FoundCrate::Itself => syn::Ident::new("crate", Span::call_site()),
            FoundCrate::Name(name) => syn::Ident::new(&name, Span::call_site()),
        },
        Err(_) => Ident::new("salvo", Span::call_site()),
    }
}

#[proc_macro_error]
#[proc_macro_derive(ToSchema, attributes(schema, aliases))]
#[doc = include_str!("../docs/derive_to_schema.md")]
pub fn derive_to_schema(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        data,
        generics,
        vis,
    } = syn::parse_macro_input!(input);

    let schema = Schema::new(&data, &attrs, &ident, &generics, &vis);
    schema.to_token_stream().into()
}

#[proc_macro_error]
#[proc_macro_attribute]
#[doc = include_str!("../docs/endpoint.md")]
pub fn endpoint(attr: TokenStream, item: TokenStream) -> TokenStream {
    endpoint::generate(attr, item)
}

#[proc_macro_error]
#[proc_macro_derive(IntoParams, attributes(param, into_params))]
#[doc = include_str!("../docs/params.md")]
pub fn into_params(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = syn::parse_macro_input!(input);

    let into_params = IntoParams {
        attrs,
        generics,
        data,
        ident,
    };

    into_params.to_token_stream().into()
}

#[proc_macro_error]
#[proc_macro_derive(ToResponse, attributes(response, content, to_schema))]
#[doc = include_str!("../docs/response.md")]
pub fn to_response(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = syn::parse_macro_input!(input);

    let response = ToResponse::new(attrs, &data, generics, ident);

    response.to_token_stream().into()
}

#[proc_macro_error]
#[proc_macro_derive(IntoResponses, attributes(response, to_schema, ref_response, to_response))]
#[doc = include_str!("../docs/into_responses.md")]
pub fn into_responses(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = syn::parse_macro_input!(input);

    let into_responses = IntoResponses {
        attributes: attrs,
        ident,
        generics,
        data,
    };

    into_responses.to_token_stream().into()
}

#[proc_macro]
#[doc = include_str!("../docs/schema.md")]
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
    let type_tree = TypeTree::from_type(&schema.ty);

    let schema = ComponentSchema::new(ComponentSchemaProps {
        features: Some(vec![Feature::Inline(schema.inline.into())]),
        type_tree: &type_tree,
        deprecated: None,
        description: None,
        object_name: "",
    });
    // schema.to_token_stream().into()
    let stream = schema.to_token_stream().into();
    println!("bbbb{}", stream);
    stream
}

/// Tokenizes slice or Vec of tokenizable items as array either with reference (`&[...]`)
/// or without correctly to OpenAPI JSON.
#[derive(Debug)]
enum Array<'a, T>
where
    T: Sized + ToTokens,
{
    Owned(Vec<T>),
    #[allow(dead_code)]
    Borrowed(&'a [T]),
}

impl<T> Array<'_, T> where T: ToTokens + Sized {}

impl<V> FromIterator<V> for Array<'_, V>
where
    V: Sized + ToTokens,
{
    fn from_iter<T: IntoIterator<Item = V>>(iter: T) -> Self {
        Self::Owned(iter.into_iter().collect())
    }
}

impl<'a, T> Deref for Array<'a, T>
where
    T: Sized + ToTokens,
{
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(vec) => vec.as_slice(),
            Self::Borrowed(slice) => slice,
        }
    }
}

impl<T> ToTokens for Array<'_, T>
where
    T: Sized + ToTokens,
{
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let values = match self {
            Self::Owned(values) => values.iter(),
            Self::Borrowed(values) => values.iter(),
        };

        tokens.append(Group::new(
            proc_macro2::Delimiter::Bracket,
            values
                .fold(Punctuated::new(), |mut punctuated, item| {
                    punctuated.push_value(item);
                    punctuated.push_punct(Punct::new(',', proc_macro2::Spacing::Alone));

                    punctuated
                })
                .to_token_stream(),
        ));
    }
}

#[derive(Debug)]
enum Deprecated {
    True,
    False,
}

impl From<bool> for Deprecated {
    fn from(bool: bool) -> Self {
        if bool {
            Self::True
        } else {
            Self::False
        }
    }
}

impl ToTokens for Deprecated {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let root = crate::root_crate();
        tokens.extend(match self {
            Self::False => quote! { #root::oapi::openapi::Deprecated::False },
            Self::True => quote! { #root::oapi::openapi::Deprecated::True },
        })
    }
}

#[derive(PartialEq, Eq, Debug)]
enum Required {
    True,
    False,
}

impl From<bool> for Required {
    fn from(bool: bool) -> Self {
        if bool {
            Self::True
        } else {
            Self::False
        }
    }
}

impl From<features::Required> for Required {
    fn from(value: features::Required) -> Self {
        let features::Required(required) = value;
        crate::Required::from(required)
    }
}

impl ToTokens for Required {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let root = crate::root_crate();
        tokens.extend(match self {
            Self::False => quote! { #root::oapi::openapi::Required::False },
            Self::True => quote! { #root::oapi::openapi::Required::True },
        })
    }
}

#[derive(Default, Debug)]
struct ExternalDocs {
    url: String,
    description: Option<String>,
}

impl Parse for ExternalDocs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        const EXPECTED_ATTRIBUTE: &str = "unexpected attribute, expected any of: url, description";

        let mut external_docs = ExternalDocs::default();

        while !input.is_empty() {
            let ident = input
                .parse::<Ident>()
                .map_err(|error| syn::Error::new(error.span(), format!("{EXPECTED_ATTRIBUTE}, {error}")))?;
            let attribute_name = &*ident.to_string();

            match attribute_name {
                "url" => {
                    external_docs.url = parse_utils::parse_next_literal_str(input)?;
                }
                "description" => {
                    external_docs.description = Some(parse_utils::parse_next_literal_str(input)?);
                }
                _ => return Err(syn::Error::new(ident.span(), EXPECTED_ATTRIBUTE)),
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(external_docs)
    }
}

impl ToTokens for ExternalDocs {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let root = crate::root_crate();
        let url = &self.url;
        tokens.extend(quote! {
            #root::oapi::openapi::external_docs::ExternalDocsBuilder::new()
                .url(#url)
        });

        if let Some(ref description) = self.description {
            tokens.extend(quote! {
                .description(#description)
            });
        }
    }
}

/// Represents OpenAPI Any value used in example and default fields.
#[derive(Clone, Debug)]
pub(self) enum AnyValue {
    String(TokenStream2),
    Json(TokenStream2),
    DefaultTrait { struct_ident: Ident, field_ident: Member },
}

impl AnyValue {
    /// Parse `json!(...)` as [`AnyValue::Json`]
    fn parse_json(input: ParseStream) -> syn::Result<Self> {
        parse_utils::parse_json_token_stream(input).map(AnyValue::Json)
    }

    fn parse_any(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Lit) {
            if input.peek(LitStr) {
                let lit_str = input.parse::<LitStr>().unwrap().to_token_stream();

                Ok(AnyValue::Json(lit_str))
            } else {
                let lit = input.parse::<Lit>().unwrap().to_token_stream();

                Ok(AnyValue::Json(lit))
            }
        } else {
            let fork = input.fork();
            let is_json = if fork.peek(syn::Ident) && fork.peek2(Token![!]) {
                let ident = fork.parse::<Ident>().unwrap();
                ident == "json"
            } else {
                false
            };

            if is_json {
                let json = parse_utils::parse_json_token_stream(input)?;

                Ok(AnyValue::Json(json))
            } else {
                let method = input.parse::<ExprPath>().map_err(|error| {
                    syn::Error::new(error.span(), "expected literal value, json!(...) or method reference")
                })?;

                Ok(AnyValue::Json(quote! { #method() }))
            }
        }
    }

    fn parse_lit_str_or_json(input: ParseStream) -> syn::Result<Self> {
        if input.peek(LitStr) {
            Ok(AnyValue::String(input.parse::<LitStr>().unwrap().to_token_stream()))
        } else {
            Ok(AnyValue::Json(parse_utils::parse_json_token_stream(input)?))
        }
    }

    fn new_default_trait(struct_ident: Ident, field_ident: Member) -> Self {
        Self::DefaultTrait {
            struct_ident,
            field_ident,
        }
    }
}

impl ToTokens for AnyValue {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        match self {
            Self::Json(json) => tokens.extend(quote! {
                serde_json::json!(#json)
            }),
            Self::String(string) => string.to_tokens(tokens),
            Self::DefaultTrait {
                struct_ident,
                field_ident,
            } => tokens.extend(quote! {
                serde_json::to_value(#struct_ident::default().#field_ident).unwrap()
            }),
        }
    }
}

trait ResultExt<T> {
    fn unwrap_or_abort(self) -> T;
    fn expect_or_abort(self, message: &str) -> T;
}

impl<T> ResultExt<T> for Result<T, syn::Error> {
    fn unwrap_or_abort(self) -> T {
        match self {
            Ok(value) => value,
            Err(error) => abort!(error.span(), format!("{error}")),
        }
    }

    fn expect_or_abort(self, message: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => abort!(error.span(), format!("{error}: {message}")),
        }
    }
}

trait OptionExt<T> {
    fn expect_or_abort(self, message: &str) -> T;
}

impl<T> OptionExt<T> for Option<T> {
    fn expect_or_abort(self, message: &str) -> T {
        self.unwrap_or_else(|| abort!(Span::call_site(), message))
    }
}
