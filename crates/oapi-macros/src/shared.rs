use std::borrow::Cow;
use std::ops::Deref;

use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Delimiter, Group, Punct, Span, TokenStream};
use proc_macro2_diagnostics::Diagnostic;
use quote::{ToTokens, TokenStreamExt, quote};
use regex::Regex;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    Attribute, ExprPath, FnArg, Ident, Lit, LitStr, Member, PatType, Receiver, Token, Type,
    TypePath,
};

use crate::feature::attributes;
use crate::{RenameRule, SerdeContainer, SerdeValue, parse_utils};

#[allow(dead_code)]
pub(crate) enum InputType<'a> {
    Request(&'a PatType),
    Depot(&'a PatType),
    Response(&'a PatType),
    FlowCtrl(&'a PatType),
    Unknown,
    Receiver(&'a Receiver),
    NoReference(&'a PatType),
}

// https://github.com/bkchr/proc-macro-crate/issues/14
pub(crate) fn oapi_crate() -> syn::Ident {
    match crate_name("salvo-oapi") {
        Ok(oapi) => match oapi {
            FoundCrate::Itself => syn::Ident::new("salvo_oapi", Span::call_site()),
            FoundCrate::Name(name) => syn::Ident::new(&name, Span::call_site()),
        },
        Err(_) => Ident::new("salvo", Span::call_site()),
    }
}
// https://github.com/bkchr/proc-macro-crate/issues/14
pub(crate) fn salvo_crate() -> syn::Ident {
    match crate_name("salvo") {
        Ok(salvo) => match salvo {
            FoundCrate::Itself => Ident::new("salvo", Span::call_site()),
            FoundCrate::Name(name) => Ident::new(&name, Span::call_site()),
        },
        Err(_) => match crate_name("salvo_core") {
            Ok(salvo) => match salvo {
                FoundCrate::Itself => Ident::new("salvo_core", Span::call_site()),
                FoundCrate::Name(name) => Ident::new(&name, Span::call_site()),
            },
            Err(_) => Ident::new("salvo", Span::call_site()),
        },
    }
}

pub(crate) fn parse_input_type(input: &FnArg) -> InputType {
    if let FnArg::Typed(p) = input {
        if let Type::Reference(ty) = &*p.ty {
            if let syn::Type::Path(nty) = &*ty.elem {
                // the last ident for path type is the real type
                // such as:
                // `::std::vec::Vec` is `Vec`
                // `Vec` is `Vec`
                let ident = &nty
                    .path
                    .segments
                    .last()
                    .expect("path segments is empty")
                    .ident;
                if ident == "Request" {
                    InputType::Request(p)
                } else if ident == "Response" {
                    InputType::Response(p)
                } else if ident == "Depot" {
                    InputType::Depot(p)
                } else if ident == "FlowCtrl" {
                    InputType::FlowCtrl(p)
                } else {
                    InputType::Unknown
                }
            } else {
                InputType::Unknown
            }
        } else {
            InputType::NoReference(p)
        }
    } else if let FnArg::Receiver(r) = input {
        InputType::Receiver(r)
    } else {
        // like self on fn
        InputType::Unknown
    }
}

pub(crate) fn omit_type_path_lifetimes(ty_path: &TypePath) -> TypePath {
    let reg = Regex::new(r"'\w+").expect("invalid regex");
    let ty_path = ty_path.into_token_stream().to_string();
    let ty_path = reg.replace_all(&ty_path, "'_");
    syn::parse_str(ty_path.as_ref()).expect("failed to parse type path")
}

pub(crate) trait TryToTokens {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()>;
    fn try_to_token_stream(&self) -> DiagResult<TokenStream> {
        let mut tokens = TokenStream::new();
        match self.try_to_tokens(&mut tokens) {
            Ok(()) => Ok(tokens),
            Err(diag) => Err(diag),
        }
    }
}

pub(crate) type DiagResult<T> = Result<T, Diagnostic>;

/// Check whether either serde `container_rule` or `field_rule` has _`default`_ attribute set.
#[inline]
pub(crate) fn is_default(
    container_rules: &Option<&SerdeContainer>,
    field_rule: &Option<&SerdeValue>,
) -> bool {
    container_rules
        .as_ref()
        .map(|rule| rule.is_default)
        .unwrap_or(false)
        || field_rule
            .as_ref()
            .map(|rule| rule.is_default)
            .unwrap_or(false)
}

/// Find `#[deprecated]` attribute from given attributes. Typically derive type attributes
/// or field attributes of struct.
pub(crate) fn get_deprecated(attributs: &[Attribute]) -> Option<crate::Deprecated> {
    if attributs
        .iter()
        .any(|attr| attr.path().is_ident("deprecated"))
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
pub(crate) fn is_required(
    field_rule: Option<&SerdeValue>,
    container_rules: Option<&SerdeContainer>,
) -> bool {
    !field_rule
        .map(|rule| rule.skip_serializing_if)
        .unwrap_or(false)
        && !field_rule.map(|rule| rule.double_option).unwrap_or(false)
        && !is_default(&container_rules, &field_rule)
}

/// Tokenizes slice or Vec of tokenizable items as array either with reference (`&[...]`)
/// or without correctly to OpenAPI JSON.
#[derive(Debug)]
pub(crate) enum Array<'a, T>
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

impl<T> Deref for Array<'_, T>
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
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let values = match self {
            Self::Owned(values) => values.iter(),
            Self::Borrowed(values) => values.iter(),
        };

        tokens.append(Group::new(
            Delimiter::Bracket,
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
pub(crate) enum Deprecated {
    True,
    False,
}

impl From<bool> for Deprecated {
    fn from(bool: bool) -> Self {
        if bool { Self::True } else { Self::False }
    }
}

impl ToTokens for Deprecated {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        tokens.extend(match self {
            Self::False => quote! { #oapi::oapi::Deprecated::False },
            Self::True => quote! { #oapi::oapi::Deprecated::True },
        })
    }
}

#[derive(PartialEq, Eq, Debug)]
pub(crate) enum Required {
    True,
    False,
}

impl From<bool> for Required {
    fn from(bool: bool) -> Self {
        if bool { Self::True } else { Self::False }
    }
}

impl From<attributes::Required> for Required {
    fn from(value: attributes::Required) -> Self {
        let attributes::Required(required) = value;
        crate::Required::from(required)
    }
}

impl ToTokens for Required {
    fn to_tokens(&self, stream: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        stream.extend(match self {
            Self::False => quote! { #oapi::oapi::Required::False },
            Self::True => quote! { #oapi::oapi::Required::True },
        })
    }
}

#[allow(dead_code)]
#[derive(Default, Debug)]
pub(crate) struct ExternalDocs {
    url: String,
    description: Option<String>,
}

impl Parse for ExternalDocs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        const EXPECTED_ATTRIBUTE: &str = "unexpected attribute, expected any of: url, description";

        let mut external_docs = ExternalDocs::default();

        while !input.is_empty() {
            let ident = input.parse::<Ident>().map_err(|error| {
                syn::Error::new(error.span(), format!("{EXPECTED_ATTRIBUTE}, {error}"))
            })?;
            let attr_name = &*ident.to_string();

            match attr_name {
                "url" => {
                    external_docs.url = parse_utils::parse_next_lit_str(input)?;
                }
                "description" => {
                    external_docs.description = Some(parse_utils::parse_next_lit_str(input)?);
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
    fn to_tokens(&self, tokens: &mut TokenStream) {
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
pub(crate) enum AnyValue {
    String(TokenStream),
    Json(TokenStream),
    DefaultTrait {
        struct_ident: Ident,
        field_ident: Member,
    },
}

impl AnyValue {
    /// Parse `json!(...)` as [`AnyValue::Json`]
    pub(crate) fn parse_json(input: ParseStream) -> syn::Result<Self> {
        parse_utils::parse_json_token_stream(input).map(AnyValue::Json)
    }

    pub(crate) fn parse_any(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Lit) {
            let punct = input.parse::<Option<Token![-]>>()?;
            let lit = input.parse::<Lit>().expect("parse_any: parse `Lit` failed");

            Ok(AnyValue::Json(quote! { #punct #lit}))
        } else {
            let fork = input.fork();
            let is_json = if fork.peek(syn::Ident) && fork.peek2(Token![!]) {
                let ident = fork.parse::<Ident>().expect("parse `Ident` failed");
                ident == "json"
            } else {
                false
            };

            if is_json {
                let json = parse_utils::parse_json_token_stream(input)?;

                Ok(AnyValue::Json(json))
            } else {
                let method = input.parse::<ExprPath>().map_err(|error| {
                    syn::Error::new(
                        error.span(),
                        "expected literal value, json!(...) or method reference",
                    )
                })?;

                Ok(AnyValue::Json(quote! { #method() }))
            }
        }
    }

    pub(crate) fn parse_lit_str_or_json(input: ParseStream) -> syn::Result<Self> {
        if input.peek(LitStr) {
            Ok(AnyValue::String(
                input
                    .parse::<LitStr>()
                    .expect("parse_lit_str_or_json: parse `LitStr` failed")
                    .to_token_stream(),
            ))
        } else {
            Ok(AnyValue::Json(parse_utils::parse_json_token_stream(input)?))
        }
    }

    pub(crate) fn new_default_trait(struct_ident: Ident, field_ident: Member) -> Self {
        Self::DefaultTrait {
            struct_ident,
            field_ident,
        }
    }
}

impl ToTokens for AnyValue {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        match self {
            Self::Json(json) => tokens.extend(quote! {
                #oapi::oapi::__private::serde_json::json!(#json)
            }),
            Self::String(string) => string.to_tokens(tokens),
            Self::DefaultTrait {
                struct_ident,
                field_ident,
            } => tokens.extend(quote! {
                #oapi::oapi::__private::serde_json::to_value(#struct_ident::default().#field_ident).unwrap()
            }),
        }
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
pub(crate) struct VariantRename;

impl Rename for VariantRename {
    fn rename(rule: &RenameRule, value: &str) -> String {
        rule.apply_to_variant(value)
    }
}

/// Can be used to perform rename on field level of a container e.g `struct`.
pub(crate) struct FieldRename;

impl Rename for FieldRename {
    fn rename(rule: &RenameRule, value: &str) -> String {
        rule.apply_to_field(value)
    }
}
