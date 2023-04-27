use std::borrow::Cow;

use proc_macro2::{Ident, TokenStream};
use proc_macro_error::abort;
use quote::{quote, ToTokens};
use syn::{GenericParam,
    parse::Parse, parse_quote, punctuated::Punctuated, spanned::Spanned, token::Comma, Attribute, Data, Fields,
    FieldsNamed, FieldsUnnamed, GenericArgument, Generics, PathArguments, Token, Type, Visibility,
};

use crate::{Array, ResultExt};

pub(crate) use self::{
    enum_schemas::*,
    feature::{FromAttributes, NamedFieldStructFeatures, UnnamedFieldStructFeatures},
    struct_schemas::*,
    xml::XmlAttr,
};

use super::{
    feature::{pop_feature_as_inner, Feature, FeaturesExt, IntoInner, Symbol},
    serde::{self, SerdeValue},
    ComponentSchema, FieldRename, VariantRename,
};
use crate::type_tree::TypeTree;

mod enum_schemas;
mod enum_variant;
mod feature;
mod struct_schemas;
mod xml;

pub(crate) struct AsSchema<'a> {
    ident: &'a Ident,
    attributes: &'a [Attribute],
    generics: &'a Generics,
    aliases: Option<Punctuated<AliasSchema, Comma>>,
    data: &'a Data,
    vis: &'a Visibility,
}

impl<'a> AsSchema<'a> {
    pub(crate) fn new(
        data: &'a Data,
        attributes: &'a [Attribute],
        ident: &'a Ident,
        generics: &'a Generics,
        vis: &'a Visibility,
    ) -> Self {
        let aliases = if generics.type_params().count() > 0 {
            parse_aliases(attributes)
        } else {
            None
        };

        Self {
            data,
            ident,
            attributes,
            generics,
            aliases,
            vis,
        }
    }
}

impl ToTokens for AsSchema<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        let ident = self.ident;
        let variant = SchemaVariant::new(
            self.data,
            self.attributes,
            ident,
            self.generics,
            None::<Vec<(TypeTree, &TypeTree)>>,
        );

        let (_, ty_generics, where_clause) = self.generics.split_for_impl();

        let schema_ty: Type = parse_quote!(#ident #ty_generics);
        let schema_children = &*TypeTree::from_type(&schema_ty).children.unwrap_or_default();

        let aliases = self.aliases.as_ref().map(|aliases| {
            let alias_schemas = aliases
                .iter()
                .map(|alias| {
                    let name = &*alias.name;
                    let alias_type_tree = TypeTree::from_type(&alias.ty);

                    let variant = SchemaVariant::new(
                        self.data,
                        self.attributes,
                        ident,
                        self.generics,
                        alias_type_tree
                            .children
                            .map(|children| children.into_iter().zip(schema_children)),
                    );
                    quote! { (#name, #variant.into()) }
                })
                .collect::<Array<TokenStream>>();

            quote! {
                fn aliases() -> Vec<(&'static str, #oapi::oapi::schema::Schema)> {
                    #alias_schemas.to_vec()
                }
            }
        });

        let type_aliases = self.aliases.as_ref().map(|aliases| {
            aliases
                .iter()
                .map(|alias| {
                    let name = quote::format_ident!("{}", alias.name);
                    let ty = &alias.ty;
                    let vis = self.vis;
                    let name_generics = alias.get_lifetimes().fold(
                        Punctuated::<&GenericArgument, Comma>::new(),
                        |mut acc, lifetime| {
                            acc.push(lifetime);
                            acc
                        },
                    );

                    if name_generics.is_empty() {
                        quote! {
                            #vis type #name = #ty;
                        }
                    } else {
                        quote! {
                            #vis type #name < #name_generics > = #ty;
                        }
                    }
                })
                .collect::<TokenStream>()
        });

        let symbol = if let Some(Symbol(symbol)) = variant.get_symbol() {
            quote! { Some(#symbol) }
        } else {
            quote! { Some(std::any::type_name::<#ident #ty_generics>().into()) }
        };

        let (impl_generics, _, _) = self.generics.split_for_impl();

        tokens.extend(quote! {
            impl #impl_generics #oapi::oapi::AsSchema for #ident #ty_generics #where_clause {
                fn symbol() -> Option<&'static str> {
                    #symbol
                }
                fn schema() -> #oapi::oapi::RefOr<#oapi::oapi::schema::Schema> {
                    #variant.into()
                }

                #aliases
            }

            #type_aliases
        })
    }
}

#[derive(Debug)]
enum SchemaVariant<'a> {
    Named(NamedStructSchema<'a>),
    Unnamed(UnnamedStructSchema<'a>),
    Enum(EnumSchema<'a>),
    Unit(UnitStructVariant),
}

impl<'a> SchemaVariant<'a> {
    pub(crate) fn new<I: IntoIterator<Item = (TypeTree<'a>, &'a TypeTree<'a>)>>(
        data: &'a Data,
        attributes: &'a [Attribute],
        ident: &'a Ident,
        generics: &'a Generics,
        aliases: Option<I>,
    ) -> SchemaVariant<'a> {
        match data {
            Data::Struct(content) => match &content.fields {
                Fields::Unnamed(fields) => {
                    let FieldsUnnamed { unnamed, .. } = fields;
                    let mut unnamed_features = attributes.parse_features::<UnnamedFieldStructFeatures>().into_inner();

                    let symbol = pop_feature_as_inner!(unnamed_features => Feature::Symbol(_v));
                    Self::Unnamed(UnnamedStructSchema {
                        struct_name: Cow::Owned(ident.to_string()),
                        attributes,
                        features: unnamed_features,
                        fields: unnamed,
                        symbol,
                    })
                }
                Fields::Named(fields) => {
                    let FieldsNamed { named, .. } = fields;
                    let mut named_features = attributes.parse_features::<NamedFieldStructFeatures>().into_inner();
                    let symbol = pop_feature_as_inner!(named_features => Feature::Symbol(_v));

                    Self::Named(NamedStructSchema {
                        struct_name: Cow::Owned(ident.to_string()),
                        attributes,
                        rename_all: named_features.pop_rename_all_feature(),
                        features: named_features,
                        fields: named,
                        generics: Some(generics),
                        symbol,
                        aliases: aliases.map(|aliases| aliases.into_iter().collect()),
                    })
                }
                Fields::Unit => Self::Unit(UnitStructVariant),
            },
            Data::Enum(content) => Self::Enum(EnumSchema::new(
                Cow::Owned(ident.to_string()),
                &content.variants,
                attributes,
            )),
            _ => abort!(
                ident.span(),
                "unexpected data type, expected syn::Data::Struct or syn::Data::Enum"
            ),
        }
    }

    fn get_symbol(&self) -> &Option<Symbol> {
        match self {
            Self::Enum(schema) => &schema.symbol,
            Self::Named(schema) => &schema.symbol,
            Self::Unnamed(schema) => &schema.symbol,
            _ => &None,
        }
    }
}

impl ToTokens for SchemaVariant<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Enum(schema) => schema.to_tokens(tokens),
            Self::Named(schema) => schema.to_tokens(tokens),
            Self::Unnamed(schema) => schema.to_tokens(tokens),
            Self::Unit(unit) => unit.to_tokens(tokens),
        }
    }
}

#[derive(Debug)]
struct UnitStructVariant;

impl ToTokens for UnitStructVariant {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        tokens.extend(quote! {
            #oapi::oapi::schema::empty()
        });
    }
}

#[derive(PartialEq, Debug)]
struct TypeTuple<'a, T>(T, &'a Ident);

#[derive(Debug)]
enum Property {
    Schema(ComponentSchema),
    WithSchema(Feature),
}

impl ToTokens for Property {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Schema(schema) => schema.to_tokens(tokens),
            Self::WithSchema(with_schema) => with_schema.to_tokens(tokens),
        }
    }
}

trait SchemaFeatureExt {
    fn split_for_title(self) -> (Vec<Feature>, Vec<Feature>);
}

impl SchemaFeatureExt for Vec<Feature> {
    fn split_for_title(self) -> (Vec<Feature>, Vec<Feature>) {
        self.into_iter()
            .partition(|feature| matches!(feature, Feature::Title(_)))
    }
}

#[inline]
fn is_not_skipped(rule: &Option<SerdeValue>) -> bool {
    rule.as_ref().map(|value| !value.skip).unwrap_or(true)
}

#[inline]
fn is_flatten(rule: &Option<SerdeValue>) -> bool {
    rule.as_ref().map(|value| value.flatten).unwrap_or(false)
}

#[derive(Debug)]
pub(crate) struct AliasSchema {
    pub(crate) name: String,
    pub(crate) ty: Type,
}

impl AliasSchema {
    fn get_lifetimes(&self) -> impl Iterator<Item = &GenericArgument> {
        fn lifetimes_from_type(ty: &Type) -> impl Iterator<Item = &GenericArgument> {
            match ty {
                Type::Path(type_path) => type_path
                    .path
                    .segments
                    .iter()
                    .flat_map(|segment| match &segment.arguments {
                        PathArguments::AngleBracketed(angle_bracketed_args) => {
                            Some(angle_bracketed_args.args.iter())
                        }
                        _ => None,
                    })
                    .flatten()
                    .flat_map(|arg| match arg {
                        GenericArgument::Type(type_argument) => {
                            lifetimes_from_type(type_argument).collect::<Vec<_>>()
                        }
                        _ => vec![arg],
                    })
                    .filter(|generic_arg| matches!(generic_arg, syn::GenericArgument::Lifetime(lifetime) if lifetime.ident != "'static")),
                _ => abort!(
                    &ty.span(),
                    "AliasSchema `get_lifetimes` only supports syn::TypePath types"
                ),
            }
        }

        lifetimes_from_type(&self.ty)
    }
}

impl Parse for AliasSchema {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name = input.parse::<Ident>()?;
        input.parse::<Token![=]>()?;

        Ok(Self {
            name: name.to_string(),
            ty: input.parse::<Type>()?,
        })
    }
}

fn parse_aliases(attributes: &[Attribute]) -> Option<Punctuated<AliasSchema, Comma>> {
    attributes
        .iter()
        .find(|attribute| attribute.path().is_ident("aliases"))
        .map(|aliases| {
            aliases
                .parse_args_with(Punctuated::<AliasSchema, Comma>::parse_terminated)
                .unwrap_or_abort()
        })
}
