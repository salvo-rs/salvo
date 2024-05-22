mod alias_schema;
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
use syn::{
    parse_quote, Attribute, Data, Fields, FieldsNamed, FieldsUnnamed, Generics,
    Type, Visibility,
};

pub(crate) use self::{
    alias_schema::*,
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
use crate::feature::{Bound, Inline, Name, SkipBound};
use crate::serde_util::SerdeValue;
use crate::{bound, DiagLevel, DiagResult, Diagnostic, TryToTokens, TypeTree};

pub(crate) struct ToSchema<'a> {
    ident: &'a Ident,
    attributes: &'a [Attribute],
    generics: &'a Generics,
    data: &'a Data,
    aliases: Option<Punctuated<AliasSchema, Comma>>,
    vis: &'a Visibility,
}

impl<'a> ToSchema<'a> {
    pub(crate) fn new(
        data: &'a Data,
        attributes: &'a [Attribute],
        ident: &'a Ident,
        generics: &'a Generics,
        vis: &'a Visibility,
    ) -> DiagResult<Self> {
        let aliases = if generics.type_params().count() > 0 {
            parse_aliases(attributes)?
        } else {
            None
        };

        Ok(Self {
            data,
            ident,
            attributes,
            generics,
            aliases,
            vis,
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
            None::<Vec<(TypeTree, &TypeTree)>>,
        )?;

        let (_, ty_generics, _) = self.generics.split_for_impl();
        let inline = variant.inline().as_ref().map(|i| i.0).unwrap_or(false);

        let schema_ty: Type = parse_quote!(#ident #ty_generics);
        let schema_children = &*TypeTree::from_type(&schema_ty)?.children.unwrap_or_default();
        // let aliases = self
        //     .aliases
        //     .as_ref()
        //     .map(|aliases| {
        //         let alias_schemas = aliases
        //             .iter()
        //             .map(|alias| {
        //                 let name = &*alias.name;
        //                 let alias_type_tree = TypeTree::from_type(&alias.ty);

        //                 SchemaVariant::new(
        //                     self.data,
        //                     self.attributes,
        //                     ident,
        //                     self.generics,
        //                     alias_type_tree?
        //                         .children
        //                         .map(|children| children.into_iter().zip(schema_children)),
        //                 )
        //                 .and_then(|variant| {
        //                     let mut alias_tokens = TokenStream::new();
        //                     match variant.try_to_tokens(&mut alias_tokens) {
        //                         Ok(_) => Ok(quote! { #alias_tokens.into().name(#name) }),
        //                         Err(diag) => Err(diag),
        //                     }
        //                 })
        //             })
        //             .collect::<DiagResult<Array<TokenStream>>>()?;

        //         DiagResult::<TokenStream>::Ok(quote! {
        //             fn aliases() -> Vec<#oapi::oapi::openapi::schema::Schema> {
        //                 #alias_schemas.to_vec()
        //             }
        //         })
        //     })
        //     .transpose()?;

        // let type_aliases = self
        //     .aliases
        //     .as_ref()
        //     .map(|aliases| {
        //         aliases
        //             .iter()
        //             .map(|alias| {
        //                 let name = quote::format_ident!("{}", alias.name);
        //                 let ty = &alias.ty;
        //                 let vis = self.vis;
        //                 let name_generics = alias.get_lifetimes()?.fold(
        //                     Punctuated::<&GenericArgument, Comma>::new(),
        //                     |mut acc, lifetime| {
        //                         acc.push(lifetime);
        //                         acc
        //                     },
        //                 );

        //                 Ok(quote! {
        //                     #vis type #name < #name_generics > = #ty;
        //                 })
        //             })
        //             .collect::<DiagResult<TokenStream>>()
        //     })
        //     .transpose()?;

        let type_aliases = self
            .aliases
            .as_ref()
            .map(|aliases| {
                aliases
                    .iter()
                    .map(|alias| {
                        let name = quote::format_ident!("{}", alias.name);
                        let ty = &alias.ty;
                        // let vis = self.vis;
                        // let name_generics = alias.get_lifetimes()?.fold(
                        //     Punctuated::<&GenericArgument, Comma>::new(),
                        //     |mut acc, lifetime| {
                        //         acc.push(lifetime);
                        //         acc
                        //     },
                        // );

                        Ok(quote! {
                            // #vis type #name < #name_generics > = #ty;
                            // #oapi::oapi::__private::inventory::submit! {
                            //     fn type_id() -> ::std::any::TypeId {
                            //         ::std::any::TypeId::of::<#ty>()
                            //     }
                            //     #oapi::oapi::schema::naming::NameRuleRegistry::save(type_id, #oapi::oapi::schema::naming::NameRule::Const(#name))
                            // }
                            #oapi::oapi::schema::naming::assign_name::<#ty>(#oapi::oapi::schema::naming::NameRule::Force(#name));
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
                quote! {
                    let name = #oapi::oapi::schema::naming::assign_name::<#ident #ty_generics>(#name_rule);
                    let ref_or = #oapi::oapi::RefOr::Ref(#oapi::oapi::Ref::new(format!("#/components/schemas/{}", name)));
                    if !components.schemas.contains_key(&name) {
                        components.schemas.insert(name.clone(), ref_or.clone());
                        let schema = #variant;
                        components.schemas.insert(name, schema);
                        #type_aliases
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

            // #oapi::oapi::__private::inventory::submit! {
            //     fn type_id() -> ::std::any::TypeId {
            //         ::std::any::TypeId::of::<#ident #ty_generics>()
            //     }
            //     #oapi::oapi::schema::naming::NameRuleRegistry::save(type_id, #name_rule)
            // }
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
    pub(crate) fn new<I: IntoIterator<Item = (TypeTree<'a>, &'a TypeTree<'a>)>>(
        data: &'a Data,
        attributes: &'a [Attribute],
        ident: &'a Ident,
        generics: &'a Generics,
        aliases: Option<I>,
    ) -> DiagResult<SchemaVariant<'a>> {
        match data {
            Data::Struct(content) => match &content.fields {
                Fields::Unnamed(fields) => {
                    let FieldsUnnamed { unnamed, .. } = fields;
                    let mut unnamed_features = attributes.parse_features::<UnnamedFieldStructFeatures>()?.into_inner();

                    let name = pop_feature_as_inner!(unnamed_features => Feature::Name(_v));
                    let inline = pop_feature_as_inner!(unnamed_features => Feature::Inline(_v));
                    Ok(Self::Unnamed(UnnamedStructSchema {
                        struct_name: Cow::Owned(ident.to_string()),
                        attributes,
                        features: unnamed_features,
                        fields: unnamed,
                        name,
                        inline,
                    }))
                }
                Fields::Named(fields) => {
                    let FieldsNamed { named, .. } = fields;
                    let mut named_features: Option<Vec<Feature>> =
                        attributes.parse_features::<NamedFieldStructFeatures>()?.into_inner();
                    let name = pop_feature_as_inner!(named_features => Feature::Name(_v));
                    let inline = pop_feature_as_inner!(named_features => Feature::Inline(_v));

                    Ok(Self::Named(NamedStructSchema {
                        struct_name: Cow::Owned(ident.to_string()),
                        attributes,
                        rename_all: named_features.pop_rename_all_feature(),
                        features: named_features,
                        fields: named,
                        generics: Some(generics),
                        name,
                        aliases: aliases.map(|aliases| aliases.into_iter().collect()),
                        inline,
                    }))
                }
                Fields::Unit => Ok(Self::Unit(UnitStructVariant)),
            },
            Data::Enum(content) => Ok(Self::Enum(EnumSchema::new(
                Cow::Owned(ident.to_string()),
                &content.variants,
                attributes,
            )?)),
            _ => Err(Diagnostic::spanned(
                ident.span(),
                DiagLevel::Error,
                "unexpected data type, expected syn::Data::Struct or syn::Data::Enum",
            )),
        }
    }

    fn name(&self) -> &Option<Name> {
        match self {
            Self::Enum(schema) => &schema.name,
            Self::Named(schema) => &schema.name,
            Self::Unnamed(schema) => &schema.name,
            _ => &None,
        }
    }
    fn inline(&self) -> &Option<Inline> {
        match self {
            Self::Enum(schema) => &schema.inline,
            Self::Named(schema) => &schema.inline,
            Self::Unnamed(schema) => &schema.inline,
            _ => &None,
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
