//! This is **private** salvo_oapi codegen library and is not used alone.
//!
//! The library contains macro implementations for salvo_oapi library. Content
//! of the library documentation is available through **salvo_oapi** library itself.
//! Consider browsing via the **salvo_oapi** crate so all links will work correctly.

#![doc(html_favicon_url = "https://salvo.rs/favicon-32x32.png")]
#![doc(html_logo_url = "https://salvo.rs/images/logo.svg")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(private_in_public, unreachable_pub)]
#![forbid(unsafe_code)]
#![warn(clippy::future_not_send)]
#![warn(rustdoc::broken_intra_doc_links)]

use std::borrow::Cow;
use std::ops::Deref;

use proc_macro::TokenStream;
use proc_macro_error::{abort, proc_macro_error};
use quote::{quote, ToTokens, TokenStreamExt};

use proc_macro2::{Group, Ident, Punct, Span, TokenStream as TokenStream2};
use syn::{
    bracketed,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    token::Bracket,
    Attribute, DeriveInput, ExprPath, Item, Lit, LitStr, Member, Token,
};

mod attribute;
mod component;
mod doc_comment;
mod endpoint;
mod feature;
mod operation;
mod parameter;
mod parse_utils;
mod response;
mod schema;
mod schema_type;
mod security_requirement;
mod serde;
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
    serde::RenameRule,
    serde::{SerdeContainer, SerdeValue},
    shared::*,
    type_tree::TypeTree,
};

#[proc_macro_error]
#[proc_macro_attribute]
pub fn endpoint(attr: TokenStream, input: TokenStream) -> TokenStream {
    let attr = syn::parse_macro_input!(attr as EndpointAttr);
    let item = parse_macro_input!(input as Item);
    match endpoint::generate(attr, item) {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_error]
#[proc_macro_derive(ToSchema, attributes(salvo))] //attributes(schema)
pub fn derive_to_schema(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        data,
        generics,
        ..
    } = syn::parse_macro_input!(input);

    ToSchema::new(&data, &attrs, &ident, &generics).to_token_stream().into()
}

#[proc_macro_error]
#[proc_macro_derive(ToParameters, attributes(salvo))] //attributes(parameter, parameters)
pub fn derive_to_parameters(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = syn::parse_macro_input!(input);

    ToParameters {
        attrs,
        generics,
        data,
        ident,
    }
    .to_token_stream()
    .into()
}

#[proc_macro_error]
#[proc_macro_derive(ToResponse, attributes(salvo))] //attributes(response, content, schema))
pub fn derive_to_response(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = syn::parse_macro_input!(input);

    ToResponse::new(attrs, &data, generics, ident).to_token_stream().into()
}

#[proc_macro_error]
#[proc_macro_derive(ToResponses, attributes(salvo))] //attributes(response, schema, ref_response, response))
pub fn to_responses(input: TokenStream) -> TokenStream {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = syn::parse_macro_input!(input);

    ToResponses {
        attributes: attrs,
        ident,
        generics,
        data,
    }
    .to_token_stream()
    .into()
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
    let type_tree = TypeTree::from_type(&schema.ty);

    let schema = ComponentSchema::new(ComponentSchemaProps {
        features: Some(vec![Feature::Inline(schema.inline.into())]),
        type_tree: &type_tree,
        deprecated: None,
        description: None,
        object_name: "",
        type_definition: false,
    });
    schema.to_token_stream().into()
}

/// Check whether either serde `container_rule` or `field_rule` has _`default`_ attribute set.
#[inline]
fn is_default(container_rules: &Option<&SerdeContainer>, field_rule: &Option<&SerdeValue>) -> bool {
    container_rules.as_ref().map(|rule| rule.is_default).unwrap_or(false)
        || field_rule.as_ref().map(|rule| rule.is_default).unwrap_or(false)
}

/// Find `#[deprecated]` attribute from given attributes. Typically derive type attributes
/// or field attributes of struct.
fn get_deprecated(attributes: &[Attribute]) -> Option<crate::Deprecated> {
    if attributes
        .iter()
        .any(|attribute| attribute.path().is_ident("deprecated"))
    {
        Some(Deprecated::True)
    } else {
        None
    }
}

/// Check whether field is required based on following rules.
///
/// * If field has not serde's `skip_serializing_if`
/// * Field is not default
fn is_required(field_rule: Option<&SerdeValue>, container_rules: Option<&SerdeContainer>) -> bool {
    !field_rule.map(|rule| rule.skip_serializing_if).unwrap_or(false)
        && !field_rule.map(|rule| rule.double_option).unwrap_or(false)
        && !is_default(&container_rules, &field_rule)
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
        let oapi = crate::oapi_crate();
        tokens.extend(match self {
            Self::False => quote! { #oapi::oapi::Deprecated::False },
            Self::True => quote! { #oapi::oapi::Deprecated::True },
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

impl From<feature::Required> for Required {
    fn from(value: feature::Required) -> Self {
        let feature::Required(required) = value;
        crate::Required::from(required)
    }
}

impl ToTokens for Required {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        let oapi = crate::oapi_crate();
        tokens.extend(match self {
            Self::False => quote! { #oapi::oapi::Required::False },
            Self::True => quote! { #oapi::oapi::Required::True },
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
        let oapi = crate::oapi_crate();
        let url = &self.url;
        tokens.extend(quote! {
            #oapi::oapi::external_docs::ExternalDocsBuilder::new()
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

pub(crate) trait Rename {
    fn rename(rule: &RenameRule, value: &str) -> String;
}

/// Performs a rename for given `value` based on given rules. If no rules were
/// provided returns [`None`]
///
/// Method accepts 3 arguments.
/// * `value` to rename.
/// * `to` Optional rename to value for fields with _`rename`_ property.
/// * `container_rule` which is used to rename containers with _`rename_all`_ property.
pub(crate) fn rename<'r, R: Rename>(
    value: &'r str,
    to: Option<Cow<'r, str>>,
    container_rule: Option<&'r RenameRule>,
) -> Option<Cow<'r, str>> {
    let rename = to.and_then(|to| if !to.is_empty() { Some(to) } else { None });

    rename.or_else(|| {
        container_rule
            .as_ref()
            .map(|container_rule| Cow::Owned(R::rename(container_rule, value)))
    })
}

/// Can be used to perform rename on container level e.g `struct`, `enum` or `enum` `variant` level.
struct VariantRename;

impl Rename for VariantRename {
    fn rename(rule: &RenameRule, value: &str) -> String {
        rule.rename_variant(value)
    }
}

/// Can be used to perform rename on field level of a container e.g `struct`.
pub(crate) struct FieldRename;

impl Rename for FieldRename {
    fn rename(rule: &RenameRule, value: &str) -> String {
        rule.rename(value)
    }
}
