mod enum_schemas;
mod enum_variant;
mod feature;
mod flattened_map_schema;
mod struct_schemas;
mod xml;

use std::borrow::Cow;

use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, quote};
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{
    Attribute, Data, DeriveInput, Fields, FieldsNamed, FieldsUnnamed, Generics, Visibility,
    parse_quote,
};

pub(crate) use self::enum_schemas::*;
pub(crate) use self::feature::{
    FromAttributes, NamedFieldStructFeatures, UnnamedFieldStructFeatures,
};
pub(crate) use self::flattened_map_schema::*;
pub(crate) use self::struct_schemas::*;
pub(crate) use self::xml::XmlAttr;
use super::{ComponentSchema, FieldRename, VariantRename};
use crate::component::ComponentDescription;
use crate::doc_comment::CommentAttributes;
use crate::feature::attributes::{Alias, Bound, Description, Inline, Name, SkipBound};
use crate::feature::{Feature, FeaturesExt, TryToTokensExt, pop_feature, pop_feature_as_inner};
use crate::schema::feature::{EnumFeatures, UnitStructFeatures};
use crate::serde_util::SerdeValue;
use crate::{DiagLevel, DiagResult, Diagnostic, IntoInner, TryToTokens, bound};

pub(crate) fn to_schema(input: DeriveInput) -> DiagResult<TokenStream> {
    let DeriveInput {
        attrs,
        ident,
        data,
        generics,
        vis,
    } = input;
    ToSchema::new(&attrs, &data, &ident, &generics, &vis).and_then(|s| s.try_to_token_stream())
}

pub(crate) struct ToSchema<'a> {
    ident: &'a Ident,
    attributes: &'a [Attribute],
    generics: &'a Generics,
    data: &'a Data,
    // aliases: Option<Punctuated<AliasSchema, Comma>>,
    // vis: &'a Visibility,
}

impl<'a> ToSchema<'a> {
    pub(crate) fn new(
        attributes: &'a [Attribute],
        data: &'a Data,
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
        let has_generics = self.generics.type_params().count() > 0;

        // Build compose context for generic types
        let compose_context = if has_generics {
            Some(crate::component::ComposeContext {
                generics_ident: quote::format_ident!("__compose_generics"),
                params: self
                    .generics
                    .type_params()
                    .map(|tp| tp.ident.to_string())
                    .collect(),
            })
        } else {
            None
        };

        // Generate the schema variant for ComposeSchema (with compose context)
        let mut compose_variant = SchemaVariant::new_with_compose(
            self.data,
            self.attributes,
            ident,
            self.generics,
            compose_context.clone(),
        )?;

        // Also generate the standard schema variant (without compose context) for non-generic
        // ToSchema use
        let mut variant = SchemaVariant::new(self.data, self.attributes, ident, self.generics)?;

        let aliases = variant.aliases();

        let (_, ty_generics, _) = self.generics.split_for_impl();
        let inline = variant.inline().as_ref().map(|i| i.0).unwrap_or(false);

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
                                name = Some(#oapi::oapi::naming::assign_name::<#ty>(#oapi::oapi::naming::NameRule::Force(#name)));
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

        // Also pop skip_bound/bound from compose variant to keep it in sync
        let _ = compose_variant.pop_skip_bound();
        let _ = compose_variant.pop_bound();

        let mut generics = bound::without_defaults(self.generics);
        if skip_bound != Some(SkipBound(true)) {
            generics = match bound {
                Some(ref predicates) => bound::with_where_predicates(&generics, predicates),
                None => bound::with_bound(
                    self.data,
                    &generics,
                    &parse_quote!(#oapi::oapi::ToSchema + #oapi::oapi::ComposeSchema + 'static),
                ),
            };
        }

        let (impl_generics, _, where_clause) = generics.split_for_impl();

        // Build ComposeSchema generics (uses ComposeSchema bound instead of ToSchema)
        let mut compose_generics = bound::without_defaults(self.generics);
        if skip_bound != Some(SkipBound(true)) {
            compose_generics = match bound {
                Some(predicates) => bound::with_where_predicates(&compose_generics, &predicates),
                None => bound::with_bound(
                    self.data,
                    &compose_generics,
                    &parse_quote!(#oapi::oapi::ComposeSchema),
                ),
            };
        }
        let (compose_impl_generics, _, compose_where_clause) = compose_generics.split_for_impl();

        let name_rule = if inline {
            None
        } else if let Some(name) = variant.name() {
            Some(quote! { #oapi::oapi::naming::NameRule::Force(#name) })
        } else {
            Some(quote! { #oapi::oapi::naming::NameRule::Auto })
        };
        let variant_tokens = variant.try_to_token_stream()?;
        let compose_variant_tokens = compose_variant.try_to_token_stream()?;

        // Generate code to ensure generic type parameters are registered before assign_name
        let generic_type_registrations: TokenStream = self
            .generics
            .type_params()
            .map(|tp| {
                let ty = &tp.ident;
                quote! {
                    let _ = <#ty as #oapi::oapi::ToSchema>::to_schema(components);
                }
            })
            .collect();

        // Generate ComposeSchema impl
        let compose_body = quote! {
            #compose_variant_tokens.into()
        };

        tokens.extend(quote! {
            impl #compose_impl_generics #oapi::oapi::ComposeSchema for #ident #ty_generics #compose_where_clause {
                fn compose(
                    components: &mut #oapi::oapi::Components,
                    __compose_generics: Vec<#oapi::oapi::RefOr<#oapi::oapi::schema::Schema>>,
                ) -> #oapi::oapi::RefOr<#oapi::oapi::schema::Schema> {
                    #compose_body
                }
            }
        });

        // Generate ToSchema impl
        let body = match name_rule {
            None => {
                quote! {
                    #variant_tokens.into()
                }
            }
            Some(name_rule) => {
                let name_tokens = if type_aliases.is_some() {
                    quote! {
                        let mut name = None;
                        #type_aliases
                        let name = name.unwrap_or_else(||#oapi::oapi::naming::assign_name::<#ident #ty_generics>(#name_rule));
                    }
                } else {
                    quote! {
                        #generic_type_registrations
                        let name = #oapi::oapi::naming::assign_name::<#ident #ty_generics>(#name_rule);
                    }
                };
                quote! {
                    #name_tokens
                    let ref_or = #oapi::oapi::RefOr::Ref(#oapi::oapi::Ref::new(format!("#/components/schemas/{}", name)));
                    if !components.schemas.contains_key(&name) {
                        components.schemas.insert(name.clone(), ref_or.clone());
                        let schema = #variant_tokens;
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
#[non_exhaustive]
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
    ) -> DiagResult<Self> {
        Self::new_with_compose(data, attributes, ident, generics, None)
    }

    pub(crate) fn new_with_compose(
        data: &'a Data,
        attributes: &'a [Attribute],
        ident: &'a Ident,
        generics: &'a Generics,
        compose_context: Option<crate::component::ComposeContext>,
    ) -> DiagResult<Self> {
        match data {
            Data::Struct(content) => match &content.fields {
                Fields::Unnamed(fields) => {
                    let FieldsUnnamed { unnamed, .. } = fields;
                    let mut unnamed_features = attributes
                        .parse_features::<UnnamedFieldStructFeatures>()?
                        .into_inner();

                    let name = pop_feature_as_inner!(unnamed_features => Feature::Name(_v));
                    let aliases = pop_feature_as_inner!(unnamed_features => Feature::Aliases(_v));
                    if generics.type_params().count() == 0
                        && !aliases.as_ref().map(|a| a.0.is_empty()).unwrap_or(true)
                    {
                        return Err(Diagnostic::spanned(
                            ident.span(),
                            DiagLevel::Error,
                            "aliases are only allowed for generic types",
                        ));
                    }

                    let inline = pop_feature_as_inner!(unnamed_features => Feature::Inline(_v));
                    let description =
                        pop_feature!(unnamed_features => Feature::Description(_)).into_inner();
                    Ok(Self::Unnamed(UnnamedStructSchema {
                        struct_name: Cow::Owned(ident.to_string()),
                        attributes,
                        description,
                        features: unnamed_features,
                        fields: unnamed,
                        name,
                        aliases: aliases.map(|a| a.0),
                        inline,
                        compose_context: compose_context.clone(),
                    }))
                }
                Fields::Named(fields) => {
                    let FieldsNamed { named, .. } = fields;
                    let mut named_features: Option<Vec<Feature>> = attributes
                        .parse_features::<NamedFieldStructFeatures>()?
                        .into_inner();

                    let generic_count = generics.type_params().count();
                    let name = pop_feature_as_inner!(named_features => Feature::Name(_v));
                    let aliases = pop_feature_as_inner!(named_features => Feature::Aliases(_v));
                    if generic_count == 0
                        && !aliases.as_ref().map(|a| a.0.is_empty()).unwrap_or(true)
                    {
                        return Err(Diagnostic::spanned(
                            ident.span(),
                            DiagLevel::Error,
                            "aliases are only allowed for generic types",
                        ));
                    }

                    let inline = pop_feature_as_inner!(named_features => Feature::Inline(_v));
                    let description =
                        pop_feature!(named_features => Feature::Description(_)).into_inner();
                    Ok(Self::Named(NamedStructSchema {
                        struct_name: Cow::Owned(ident.to_string()),
                        attributes,
                        description,
                        rename_all: named_features.pop_rename_all_feature(),
                        features: named_features,
                        fields: named,
                        generics: Some(generics),
                        name,
                        aliases: aliases.map(|a| a.0),
                        inline,
                        compose_context: compose_context.clone(),
                    }))
                }
                Fields::Unit => Ok(Self::Unit(UnitStructVariant::new(attributes)?)),
            },
            Data::Enum(content) => {
                let mut enum_features: Option<Vec<Feature>> =
                    attributes.parse_features::<EnumFeatures>()?.into_inner();
                let aliases = pop_feature_as_inner!(enum_features => Feature::Aliases(_v));
                Ok(Self::Enum(EnumSchema::new(
                    Cow::Owned(ident.to_string()),
                    &content.variants,
                    attributes,
                    aliases.map(|a| a.0),
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
struct UnitStructVariant(TokenStream);

impl UnitStructVariant {
    fn new(attributes: &[Attribute]) -> DiagResult<Self> {
        let oapi = crate::oapi_crate();
        let mut tokens = quote! {
            #oapi::oapi::Object::new()
                .schema_type(#oapi::oapi::schema::SchemaType::AnyValue)
                .default_value(#oapi::oapi::__private::serde_json::Value::Null)
        };

        let mut features = attributes
            .parse_features::<UnitStructFeatures>()?
            .into_inner()
            .unwrap_or_default();

        let description = pop_feature!(features => Feature::Description(_)).and_then(|f| match f {
            Feature::Description(d) => Some(d),
            _ => None,
        });

        let comments = CommentAttributes::from_attributes(attributes);
        let description = description
            .as_ref()
            .map(ComponentDescription::Description)
            .or(Some(ComponentDescription::CommentAttributes(&comments)));

        description.to_tokens(&mut tokens);
        tokens.extend(features.try_to_token_stream()?);

        Ok(Self(tokens))
    }
}

impl ToTokens for UnitStructVariant {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
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
