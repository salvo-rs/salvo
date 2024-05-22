mod enum_schemas;
mod enum_variant;
mod feature;
mod flattened_map_schema;
mod struct_schemas;
mod xml;

use std::borrow::Cow;

use proc_macro2::{Ident, TokenStream};
use quote::{quote, ToTokens};
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{parse_quote, Attribute, Data, Fields, FieldsNamed, FieldsUnnamed, Generics, Type, Visibility};

pub(crate) use self::{
    enum_schemas::*,
    feature::{FromAttributes, NamedFieldStructFeatures, UnnamedFieldStructFeatures},
    flattened_map_schema::*,
    struct_schemas::*,
    xml::XmlAttr,
};

use super::{
    feature::{pop_feature_as_inner, Feature, FeaturesExt, IntoInner},
    ComponentSchema, FieldRename, VariantRename,
};
use crate::feature::{Alias, Bound, Inline, Name, SkipBound};
use crate::schema::feature::EnumFeatures;
use crate::serde_util::SerdeValue;
use crate::{bound, DiagLevel, DiagResult, Diagnostic, TryToTokens, TypeTree};

pub(crate) struct ToSchema<'a> {
    ident: &'a Ident,
    attributes: &'a [Attribute],
    generics: &'a Generics,
    data: &'a Data,
    // aliases: Option<Punctuated<AliasSchema, Comma>>,
    //vis: &'a Visibility,
}

impl<'a> ToSchema<'a> {
    pub(crate) fn new(
        data: &'a Data,
        attributes: &'a [Attribute],
        ident: &'a Ident,
        generics: &'a Generics,
        _vis: &'a Visibility,
    ) -> DiagResult<Self> {
        Ok(Self {
            data,
            ident,
            attributes,
            generics,
            // aliases,
            // vis,
        })
    }
}

impl TryToTokens for ToSchema<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let ident = self.ident;
        let mut variant = SchemaVariant::new(
            self.data,
            self.attributes,
            ident,
            self.generics,
            // None::<Vec<(TypeTree, &TypeTree)>>,
        )?;

        let aliases = variant.aliases();

        let (_, ty_generics, _) = self.generics.split_for_impl();
        let inline = variant.inline().as_ref().map(|i| i.0).unwrap_or(false);

        let schema_ty: Type = parse_quote!(#ident #ty_generics);
        let schema_children = &*TypeTree::from_type(&schema_ty)?.children.unwrap_or_default();

        let type_aliases = aliases
            .as_ref()
            .map(|aliases| {
                aliases
                    .iter()
                    .map(|alias| {
                        let name = quote::format_ident!("{}", alias.name).to_string();
                        let ty = &alias.ty;

                        Ok(quote! {
                            if ::std::any::TypeId::of::<Self>() == ::std::any::TypeId::of::<#ty>() {
                                name = Some(#oapi::oapi::schema::naming::assign_name::<#ty>(#oapi::oapi::schema::naming::NameRule::Force(#name)));
                            }
                        })
                    })
                    .collect::<DiagResult<TokenStream>>()
            })
            .transpose()?;

        let skip_bound = variant.pop_skip_bound();
        let bound = if skip_bound == Some(SkipBound(true)) {
            None
        } else {
            variant.pop_bound().map(|b| b.0)
        };

        let mut generics = bound::without_defaults(self.generics);
        if skip_bound != Some(SkipBound(true)) {
            generics = match bound {
                Some(predicates) => bound::with_where_predicates(&generics, &predicates),
                None => bound::with_bound(self.data, &generics, parse_quote!(#oapi::oapi::ToSchema + 'static)),
            };
        }

        let (impl_generics, _, where_clause) = generics.split_for_impl();

        let name_rule = if inline {
            None
        } else if let Some(name) = variant.name() {
            let name = name.0.path.to_token_stream();
            let name = quote!(#name).to_string();
            Some(quote! { #oapi::oapi::schema::naming::NameRule::Force(#name) })
        } else {
            Some(quote! { #oapi::oapi::schema::naming::NameRule::Auto })
        };
        let variant = variant.try_to_token_stream()?;
        let body = match name_rule {
            None => {
                quote! {
                    #variant.into()
                }
            }
            Some(name_rule) => {
                let name_tokens = if type_aliases.is_some() {
                    quote! {
                        let mut name = None;
                        #type_aliases
                        let name = name.unwrap_or_else(||#oapi::oapi::schema::naming::assign_name::<#ident #ty_generics>(#name_rule));
                    }
                } else {
                    quote! {
                        let name = #oapi::oapi::schema::naming::assign_name::<#ident #ty_generics>(#name_rule);
                    }
                };
                quote! {
                    #name_tokens
                    let ref_or = #oapi::oapi::RefOr::Ref(#oapi::oapi::Ref::new(format!("#/components/schemas/{}", name)));
                    if !components.schemas.contains_key(&name) {
                        components.schemas.insert(name.clone(), ref_or.clone());
                        let schema = #variant;
                        components.schemas.insert(name, schema);
                    }
                    ref_or
                }
            }
        };
        tokens.extend(quote!{
            impl #impl_generics #oapi::oapi::ToSchema for #ident #ty_generics #where_clause {
                fn to_schema(components: &mut #oapi::oapi::Components) -> #oapi::oapi::RefOr<#oapi::oapi::schema::Schema> {
                    #body
                }
            }
        });
        Ok(())
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
    pub(crate) fn new(
        data: &'a Data,
        attributes: &'a [Attribute],
        ident: &'a Ident,
        generics: &'a Generics,
    ) -> DiagResult<SchemaVariant<'a>> {
        match data {
            Data::Struct(content) => match &content.fields {
                Fields::Unnamed(fields) => {
                    let FieldsUnnamed { unnamed, .. } = fields;
                    let mut unnamed_features = attributes.parse_features::<UnnamedFieldStructFeatures>()?.into_inner();

                    let name = pop_feature_as_inner!(unnamed_features => Feature::Name(_v));
                    let aliases = pop_feature_as_inner!(unnamed_features => Feature::Aliases(_v));
                    if generics.type_params().count() == 0 && !aliases.as_ref().map(|a| a.0.is_empty()).unwrap_or(true)
                    {
                        return Err(Diagnostic::spanned(
                            ident.span(),
                            DiagLevel::Error,
                            "aliases are only allowed for generic types",
                        ));
                    }
                    let inline = pop_feature_as_inner!(unnamed_features => Feature::Inline(_v));
                    Ok(Self::Unnamed(UnnamedStructSchema {
                        struct_name: Cow::Owned(ident.to_string()),
                        attributes,
                        features: unnamed_features,
                        fields: unnamed,
                        name,
                        aliases: aliases.map(|a| a.0),
                        inline,
                    }))
                }
                Fields::Named(fields) => {
                    let FieldsNamed { named, .. } = fields;
                    let mut named_features: Option<Vec<Feature>> =
                        attributes.parse_features::<NamedFieldStructFeatures>()?.into_inner();
                    let name = pop_feature_as_inner!(named_features => Feature::Name(_v));
                    let aliases = pop_feature_as_inner!(named_features => Feature::Aliases(_v));
                    if generics.type_params().count() == 0 && !aliases.as_ref().map(|a| a.0.is_empty()).unwrap_or(true)
                    {
                        return Err(Diagnostic::spanned(
                            ident.span(),
                            DiagLevel::Error,
                            "aliases are only allowed for generic types",
                        ));
                    }
                    let inline = pop_feature_as_inner!(named_features => Feature::Inline(_v));

                    Ok(Self::Named(NamedStructSchema {
                        struct_name: Cow::Owned(ident.to_string()),
                        attributes,
                        rename_all: named_features.pop_rename_all_feature(),
                        features: named_features,
                        fields: named,
                        generics: Some(generics),
                        name,
                        aliases: aliases.map(|a| a.0),
                        inline,
                    }))
                }
                Fields::Unit => Ok(Self::Unit(UnitStructVariant)),
            },
            Data::Enum(content) => {
                let mut enum_features: Option<Vec<Feature>> = attributes.parse_features::<EnumFeatures>()?.into_inner();
                let aliases = pop_feature_as_inner!(enum_features => Feature::Aliases(_v));
                Ok(Self::Enum(EnumSchema::new(
                    Cow::Owned(ident.to_string()),
                    &content.variants,
                    attributes,
                    Some(generics),
                )?))
            }
            _ => Err(Diagnostic::spanned(
                ident.span(),
                DiagLevel::Error,
                "unexpected data type, expected syn::Data::Struct or syn::Data::Enum",
            )),
        }
    }

    fn name(&self) -> Option<&Name> {
        match self {
            Self::Enum(schema) => schema.name.as_ref(),
            Self::Named(schema) => schema.name.as_ref(),
            Self::Unnamed(schema) => schema.name.as_ref(),
            _ => None,
        }
    }
    fn inline(&self) -> Option<&Inline> {
        match self {
            Self::Enum(schema) => schema.inline.as_ref(),
            Self::Named(schema) => schema.inline.as_ref(),
            Self::Unnamed(schema) => schema.inline.as_ref(),
            _ => None,
        }
    }
    fn aliases(&self) -> Option<&Punctuated<Alias, Comma>> {
        match self {
            Self::Enum(schema) => schema.aliases.as_ref(),
            Self::Named(schema) => schema.aliases.as_ref(),
            Self::Unnamed(schema) => schema.aliases.as_ref(),
            _ => None,
        }
    }
    fn pop_skip_bound(&mut self) -> Option<SkipBound> {
        match self {
            Self::Enum(schema) => schema.pop_skip_bound(),
            Self::Named(schema) => schema.pop_skip_bound(),
            Self::Unnamed(schema) => schema.pop_skip_bound(),
            _ => None,
        }
    }
    fn pop_bound(&mut self) -> Option<Bound> {
        match self {
            Self::Enum(schema) => schema.pop_bound(),
            Self::Named(schema) => schema.pop_bound(),
            Self::Unnamed(schema) => schema.pop_bound(),
            _ => None,
        }
    }
}

impl TryToTokens for SchemaVariant<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        match self {
            Self::Enum(schema) => schema.try_to_tokens(tokens),
            Self::Named(schema) => schema.try_to_tokens(tokens),
            Self::Unnamed(schema) => schema.try_to_tokens(tokens),
            Self::Unit(unit) => {
                unit.to_tokens(tokens);
                Ok(())
            }
        }
    }
}

#[derive(Debug)]
struct UnitStructVariant;

impl ToTokens for UnitStructVariant {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        let oapi = crate::oapi_crate();
        stream.extend(quote! {
            #oapi::oapi::schema::empty()
        });
    }
}

#[derive(Debug)]
enum Property {
    Schema(ComponentSchema),
    SchemaWith(Feature),
    FlattenedMap(FlattenedMapSchema),
}

impl TryToTokens for Property {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        match self {
            Self::Schema(schema) => {
                schema.to_tokens(tokens);
                Ok(())
            }
            Self::FlattenedMap(schema) => {
                schema.to_tokens(tokens);
                Ok(())
            }
            Self::SchemaWith(with_schema) => with_schema.try_to_tokens(tokens),
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
fn is_not_skipped(rule: Option<&SerdeValue>) -> bool {
    rule.map(|value| !value.skip).unwrap_or(true)
}

#[inline]
fn is_flatten(rule: Option<&SerdeValue>) -> bool {
    rule.map(|value| value.flatten).unwrap_or(false)
}
