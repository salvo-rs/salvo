use std::borrow::Cow;

use proc_macro2::{Ident, TokenStream};
use proc_macro_error::abort;
use quote::{quote, ToTokens};
use syn::{Attribute, Data, Fields, FieldsNamed, FieldsUnnamed, Generics};

mod enum_schemas;
mod enum_variant;
mod feature;
mod flattened_map_schema;
mod struct_schemas;
mod xml;

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
use crate::feature::{Inline, Symbol};
use crate::serde_util::SerdeValue;

pub(crate) struct ToSchema<'a> {
    ident: &'a Ident,
    attributes: &'a [Attribute],
    generics: &'a Generics,
    data: &'a Data,
    // vis: &'a Visibility,
}

impl<'a> ToSchema<'a> {
    pub(crate) fn new(
        data: &'a Data,
        attributes: &'a [Attribute],
        ident: &'a Ident,
        generics: &'a Generics,
        // vis: &'a Visibility,
    ) -> Self {
        Self {
            data,
            ident,
            attributes,
            generics,
            // vis,
        }
    }
}

impl ToTokens for ToSchema<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        let ident = self.ident;
        let variant = SchemaVariant::new(self.data, self.attributes, ident, self.generics);

        let (_, ty_generics, where_clause) = self.generics.split_for_impl();

        let inline = variant.inline().as_ref().map(|i| i.0).unwrap_or(false);
        let symbol = if inline {
            None
        } else if let Some(symbol) = variant.symbol() {
            if self.generics.type_params().next().is_none() {
                Some(quote! { #symbol.to_string().replace(" :: ", ".") })
            } else {
                Some(quote! {
                   {
                       let full_name = std::any::type_name::<#ident #ty_generics>();
                       if let Some((_, args)) = full_name.split_once('<') {
                           format!("{}<{}", #symbol, args)
                       } else {
                           full_name.into()
                       }
                   }
                })
            }
        } else {
            Some(quote! { std::any::type_name::<#ident #ty_generics>().replace("::", ".") })
        };

        let (impl_generics, _, _) = self.generics.split_for_impl();

        let body = match symbol {
            None => {
                quote! {
                    #variant.into()
                }
            }
            Some(symbol) => {
                quote! {
                    let schema = #variant;
                    components.schemas.insert(#symbol, schema.into());
                    #oapi::oapi::RefOr::Ref(#oapi::oapi::Ref::new(format!("#/components/schemas/{}", #symbol)))
                }
            }
        };
        tokens.extend(quote!{
            impl #impl_generics #oapi::oapi::ToSchema for #ident #ty_generics #where_clause {
                fn to_schema(components: &mut #oapi::oapi::Components) -> #oapi::oapi::RefOr<#oapi::oapi::schema::Schema> {
                    #body
                }
            }
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
    pub(crate) fn new(
        data: &'a Data,
        attributes: &'a [Attribute],
        ident: &'a Ident,
        generics: &'a Generics,
    ) -> SchemaVariant<'a> {
        match data {
            Data::Struct(content) => match &content.fields {
                Fields::Unnamed(fields) => {
                    let FieldsUnnamed { unnamed, .. } = fields;
                    let mut unnamed_features = attributes.parse_features::<UnnamedFieldStructFeatures>().into_inner();

                    let symbol = pop_feature_as_inner!(unnamed_features => Feature::Symbol(_v));
                    let inline = pop_feature_as_inner!(unnamed_features => Feature::Inline(_v));
                    Self::Unnamed(UnnamedStructSchema {
                        struct_name: Cow::Owned(ident.to_string()),
                        attributes,
                        features: unnamed_features,
                        fields: unnamed,
                        symbol,
                        inline,
                    })
                }
                Fields::Named(fields) => {
                    let FieldsNamed { named, .. } = fields;
                    let mut named_features = attributes.parse_features::<NamedFieldStructFeatures>().into_inner();
                    let symbol = pop_feature_as_inner!(named_features => Feature::Symbol(_v));
                    let inline = pop_feature_as_inner!(named_features => Feature::Inline(_v));

                    Self::Named(NamedStructSchema {
                        struct_name: Cow::Owned(ident.to_string()),
                        attributes,
                        rename_all: named_features.pop_rename_all_feature(),
                        features: named_features,
                        fields: named,
                        generics: Some(generics),
                        symbol,
                        inline,
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

    fn symbol(&self) -> &Option<Symbol> {
        match self {
            Self::Enum(schema) => &schema.symbol,
            Self::Named(schema) => &schema.symbol,
            Self::Unnamed(schema) => &schema.symbol,
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
    SchemaWith(Feature),
    FlattenedMap(FlattenedMapSchema),
}

impl ToTokens for Property {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Schema(schema) => schema.to_tokens(tokens),
            Self::FlattenedMap(schema) => schema.to_tokens(tokens),
            Self::SchemaWith(with_schema) => with_schema.to_tokens(tokens),
        }
    }
}

trait SchemaFeatureExt {
    fn split_for_symbol(self) -> (Vec<Feature>, Vec<Feature>);
}

impl SchemaFeatureExt for Vec<Feature> {
    fn split_for_symbol(self) -> (Vec<Feature>, Vec<Feature>) {
        self.into_iter()
            .partition(|feature| matches!(feature, Feature::Symbol(_)))
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
