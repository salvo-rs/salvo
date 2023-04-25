use std::borrow::Cow;

use proc_macro2::{Ident, TokenStream};
use proc_macro_error::abort;
use quote::{format_ident, quote, ToTokens};
use syn::{
    parse::Parse, parse_quote, punctuated::Punctuated, spanned::Spanned, token::Comma, Attribute, Data, Field, Fields,
    FieldsNamed, FieldsUnnamed, GenericArgument, Generics, Path, PathArguments, Token, Type, Variant, Visibility,
};

use crate::{
    component::features::{Example, Rename},
    doc_comment::CommentAttributes,
    Array, ResultExt,
};

use self::{
    enum_variant::{
        AdjacentlyTaggedEnum, CustomEnum, Enum, ObjectVariant, SimpleEnumVariant, TaggedEnum, UntaggedEnum,
    },
    features::{
        ComplexEnumFeatures, EnumFeatures, EnumNamedFieldVariantFeatures, EnumUnnamedFieldVariantFeatures,
        FromAttributes, NamedFieldFeatures, NamedFieldStructFeatures, UnnamedFieldStructFeatures,
    },
};

use super::{
    features::{
        parse_features, pop_feature, pop_feature_as_inner, As, Feature, FeaturesExt, IntoInner, RenameAll, ToTokensExt,
    },
    serde::{self, SerdeContainer, SerdeEnumRepr, SerdeValue},
    ComponentSchema, FieldRename, TypeTree, ValueType, VariantRename,
};

mod enum_variant;
mod features;
pub mod xml;

pub struct AsSchema<'a> {
    ident: &'a Ident,
    attributes: &'a [Attribute],
    generics: &'a Generics,
    aliases: Option<Punctuated<AliasSchema, Comma>>,
    data: &'a Data,
    vis: &'a Visibility,
}

impl<'a> AsSchema<'a> {
    pub fn new(
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

                    quote! {
                        #vis type #name < #name_generics > = #ty;
                    }
                })
                .collect::<TokenStream>()
        });

        let name = if let Some(schema_as) = variant.get_schema_as() {
            format_path_ref(&schema_as.0.path)
        } else {
            ident.to_string()
        };

        let (impl_generics, _, _) = self.generics.split_for_impl();

        tokens.extend(quote! {
            impl #impl_generics #oapi::oapi::AsSchema for #ident #ty_generics #where_clause {
                fn schema() -> (Option<&'static str>, #oapi::oapi::RefOr<#oapi::oapi::schema::Schema>) {
                    (Some(#name), #variant.into())
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
    pub fn new<I: IntoIterator<Item = (TypeTree<'a>, &'a TypeTree<'a>)>>(
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

                    let schema_as = pop_feature_as_inner!(unnamed_features => Feature::As(_v));
                    Self::Unnamed(UnnamedStructSchema {
                        struct_name: Cow::Owned(ident.to_string()),
                        attributes,
                        features: unnamed_features,
                        fields: unnamed,
                        schema_as,
                    })
                }
                Fields::Named(fields) => {
                    let FieldsNamed { named, .. } = fields;
                    let mut named_features = attributes.parse_features::<NamedFieldStructFeatures>().into_inner();
                    let schema_as = pop_feature_as_inner!(named_features => Feature::As(_v));

                    Self::Named(NamedStructSchema {
                        struct_name: Cow::Owned(ident.to_string()),
                        attributes,
                        rename_all: named_features.pop_rename_all_feature(),
                        features: named_features,
                        fields: named,
                        generics: Some(generics),
                        schema_as,
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

    fn get_schema_as(&self) -> &Option<As> {
        match self {
            Self::Enum(schema) => &schema.schema_as,
            Self::Named(schema) => &schema.schema_as,
            Self::Unnamed(schema) => &schema.schema_as,
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

#[derive(Debug)]
pub struct NamedStructSchema<'a> {
    pub struct_name: Cow<'a, str>,
    pub fields: &'a Punctuated<Field, Comma>,
    pub attributes: &'a [Attribute],
    pub features: Option<Vec<Feature>>,
    pub rename_all: Option<RenameAll>,
    pub generics: Option<&'a Generics>,
    pub aliases: Option<Vec<(TypeTree<'a>, &'a TypeTree<'a>)>>,
    pub schema_as: Option<As>,
}

struct NamedStructFieldOptions<'a> {
    property: Property,
    rename_field_value: Option<Cow<'a, str>>,
    required: Option<super::features::Required>,
    is_option: bool,
}

impl NamedStructSchema<'_> {
    fn field_as_schema_property<R>(
        &self,
        field: &Field,
        container_rules: &Option<SerdeContainer>,
        yield_: impl FnOnce(NamedStructFieldOptions<'_>) -> R,
    ) -> R {
        let type_tree = &mut TypeTree::from_type(&field.ty);
        if let Some(aliases) = &self.aliases {
            for (new_generic, old_generic_matcher) in aliases.iter() {
                if let Some(generic_match) = type_tree.find_mut(old_generic_matcher) {
                    *generic_match = new_generic.clone();
                }
            }
        }

        let mut field_features = field.attrs.parse_features::<NamedFieldFeatures>().into_inner();

        let schema_default = self
            .features
            .as_ref()
            .map(|features| features.iter().any(|f| matches!(f, Feature::Default(_))))
            .unwrap_or(false);
        let serde_default = container_rules.as_ref().map(|rules| rules.is_default).unwrap_or(false);

        if schema_default || serde_default {
            let features_inner = field_features.get_or_insert(vec![]);
            if !features_inner.iter().any(|f| matches!(f, Feature::Default(_))) {
                let field_ident = field.ident.as_ref().unwrap().to_owned();
                let struct_ident = format_ident!("{}", &self.struct_name);
                features_inner.push(Feature::Default(crate::features::Default::new_default_trait(
                    struct_ident,
                    field_ident.into(),
                )));
            }
        }

        let rename_field = pop_feature!(field_features => Feature::Rename(_)).and_then(|feature| match feature {
            Feature::Rename(rename) => Some(Cow::Owned(rename.into_value())),
            _ => None,
        });

        let deprecated = super::get_deprecated(&field.attrs);
        let value_type = field_features
            .as_mut()
            .and_then(|features| features.pop_value_type_feature());
        let override_type_tree = value_type.as_ref().map(|value_type| value_type.as_type_tree());
        let comments = CommentAttributes::from_attributes(&field.attrs);
        let with_schema = pop_feature!(field_features => Feature::SchemaWith(_));
        let required = pop_feature_as_inner!(field_features => Feature::Required(_v));
        let type_tree = override_type_tree.as_ref().unwrap_or(type_tree);
        let is_option = type_tree.is_option();

        yield_(NamedStructFieldOptions {
            property: if let Some(with_schema) = with_schema {
                Property::WithSchema(with_schema)
            } else {
                Property::Schema(ComponentSchema::new(super::ComponentSchemaProps {
                    type_tree,
                    features: field_features,
                    description: Some(&comments),
                    deprecated: deprecated.as_ref(),
                    object_name: self.struct_name.as_ref(),
                }))
            },
            rename_field_value: rename_field,
            required,
            is_option,
        })
    }
}

impl ToTokens for NamedStructSchema<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        let container_rules = serde::parse_container(self.attributes);

        let object_tokens = self
            .fields
            .iter()
            .filter_map(|field| {
                let field_rule = serde::parse_value(&field.attrs);

                if is_not_skipped(&field_rule) && !is_flatten(&field_rule) {
                    Some((field, field_rule))
                } else {
                    None
                }
            })
            .fold(
                quote! { #oapi::oapi::Object::new() },
                |mut object_tokens, (field, field_rule)| {
                    let mut field_name = &*field.ident.as_ref().unwrap().to_string();

                    if field_name.starts_with("r#") {
                        field_name = &field_name[2..];
                    }

                    self.field_as_schema_property(
                        field,
                        &container_rules,
                        |NamedStructFieldOptions {
                             property,
                             rename_field_value,
                             required,
                             is_option,
                         }| {
                            let rename_to = field_rule
                                .as_ref()
                                .and_then(|field_rule| field_rule.rename.as_deref().map(Cow::Borrowed))
                                .or(rename_field_value);
                            let rename_all = container_rules
                                .as_ref()
                                .and_then(|container_rule| container_rule.rename_all.as_ref())
                                .or_else(|| self.rename_all.as_ref().map(|rename_all| rename_all.as_rename_rule()));

                            let name = super::rename::<FieldRename>(field_name, rename_to, rename_all)
                                .unwrap_or(Cow::Borrowed(field_name));

                            object_tokens.extend(quote! {
                                .property(#name, #property)
                            });

                            if let Property::Schema(_) = property {
                                if (!is_option && super::is_required(field_rule.as_ref(), container_rules.as_ref()))
                                    || required
                                        .as_ref()
                                        .map(super::features::Required::is_true)
                                        .unwrap_or(false)
                                {
                                    object_tokens.extend(quote! {
                                        .required(#name)
                                    })
                                }
                            }

                            object_tokens
                        },
                    )
                },
            );

        let flatten_fields: Vec<&Field> = self
            .fields
            .iter()
            .filter(|field| {
                let field_rule = serde::parse_value(&field.attrs);
                is_flatten(&field_rule)
            })
            .collect();

        if !flatten_fields.is_empty() {
            tokens.extend(quote! {
                #oapi::oapi::AllOf::new()
            });

            for field in flatten_fields {
                self.field_as_schema_property(
                    field,
                    &container_rules,
                    |NamedStructFieldOptions { property, .. }| {
                        tokens.extend(quote! { .item(#property) });
                    },
                )
            }

            tokens.extend(quote! {
                .item(#object_tokens)
            })
        } else {
            tokens.extend(object_tokens)
        }

        if let Some(deprecated) = super::get_deprecated(self.attributes) {
            tokens.extend(quote! { .deprecated(#deprecated) });
        }

        if let Some(struct_features) = self.features.as_ref() {
            tokens.extend(struct_features.to_token_stream())
        }

        let description = CommentAttributes::from_attributes(self.attributes).as_formatted_string();
        if !description.is_empty() {
            tokens.extend(quote! {
                .description(#description)
            })
        }
    }
}

#[derive(Debug)]
struct UnnamedStructSchema<'a> {
    struct_name: Cow<'a, str>,
    fields: &'a Punctuated<Field, Comma>,
    attributes: &'a [Attribute],
    features: Option<Vec<Feature>>,
    schema_as: Option<As>,
}

impl ToTokens for UnnamedStructSchema<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        let fields_len = self.fields.len();
        let first_field = self.fields.first().unwrap();
        let first_part = &TypeTree::from_type(&first_field.ty);

        let mut is_object = matches!(first_part.value_type, ValueType::Object);

        let all_fields_are_same = fields_len == 1
            || self.fields.iter().skip(1).all(|field| {
                let schema_part = &TypeTree::from_type(&field.ty);

                first_part == schema_part
            });

        let deprecated = super::get_deprecated(self.attributes);
        if all_fields_are_same {
            let mut unnamed_struct_features = self.features.clone();
            let value_type = unnamed_struct_features
                .as_mut()
                .and_then(|features| features.pop_value_type_feature());
            let override_type_tree = value_type.as_ref().map(|value_type| value_type.as_type_tree());

            if override_type_tree.is_some() {
                is_object = override_type_tree
                    .as_ref()
                    .map(|override_type| matches!(override_type.value_type, ValueType::Object))
                    .unwrap_or_default();
            }

            if fields_len == 1 {
                if let Some(ref mut features) = unnamed_struct_features {
                    if pop_feature!(features => Feature::Default(crate::features::Default(None))).is_some() {
                        let struct_ident = format_ident!("{}", &self.struct_name);
                        let index: syn::Index = 0.into();
                        features.push(Feature::Default(crate::features::Default::new_default_trait(
                            struct_ident,
                            index.into(),
                        )));
                    }
                }
            }

            tokens.extend(
                ComponentSchema::new(super::ComponentSchemaProps {
                    type_tree: override_type_tree.as_ref().unwrap_or(first_part),
                    features: unnamed_struct_features,
                    description: None,
                    deprecated: deprecated.as_ref(),
                    object_name: self.struct_name.as_ref(),
                })
                .to_token_stream(),
            );
        } else {
            // Struct that has multiple unnamed fields is serialized to array by default with serde.
            // See: https://serde.rs/json.html
            // Typically OpenAPI does not support multi type arrays thus we simply consider the case
            // as generic object array
            tokens.extend(quote! {
                #oapi::oapi::Object::new()
            });

            if let Some(deprecated) = deprecated {
                tokens.extend(quote! { .deprecated(#deprecated) });
            }

            if let Some(ref attrs) = self.features {
                tokens.extend(attrs.to_token_stream())
            }
        };

        let description = CommentAttributes::from_attributes(self.attributes).as_formatted_string();
        if !description.is_empty() && !is_object {
            tokens.extend(quote! {
                .description(#description)
            })
        }

        if fields_len > 1 {
            tokens.extend(quote! { .max_items(Some(#fields_len)).min_items(Some(#fields_len)) })
        }
    }
}

#[derive(Debug)]
pub struct EnumSchema<'a> {
    schema_type: EnumSchemaType<'a>,
    schema_as: Option<As>,
}

impl<'e> EnumSchema<'e> {
    pub fn new(enum_name: Cow<'e, str>, variants: &'e Punctuated<Variant, Comma>, attributes: &'e [Attribute]) -> Self {
        if variants.iter().all(|variant| matches!(variant.fields, Fields::Unit)) {
            #[cfg(feature = "repr")]
            {
                attributes
                    .iter()
                    .find_map(|attribute| {
                        if attribute.path().is_ident("repr") {
                            attribute.parse_args::<syn::TypePath>().ok()
                        } else {
                            None
                        }
                    })
                    .map(|enum_type| {
                        let mut repr_enum_features = features::parse_schema_features_with(attributes, |input| {
                            Ok(parse_features!(
                                input as super::features::Example,
                                super::features::Default,
                                super::features::Title,
                                As
                            ))
                        })
                        .unwrap_or_default();

                        let schema_as = pop_feature_as_inner!(repr_enum_features => Feature::As(_v));
                        Self {
                            schema_type: EnumSchemaType::Repr(ReprEnum {
                                variants,
                                attributes,
                                enum_type,
                                enum_features: repr_enum_features,
                            }),
                            schema_as,
                        }
                    })
                    .unwrap_or_else(|| {
                        let mut simple_enum_features = attributes
                            .parse_features::<EnumFeatures>()
                            .into_inner()
                            .unwrap_or_default();
                        let schema_as = pop_feature_as_inner!(simple_enum_features => Feature::As(_v));
                        let rename_all = simple_enum_features.pop_rename_all_feature();

                        Self {
                            schema_type: EnumSchemaType::Simple(SimpleEnum {
                                attributes,
                                variants,
                                enum_features: simple_enum_features,
                                rename_all,
                            }),
                            schema_as,
                        }
                    })
            }

            #[cfg(not(feature = "repr"))]
            {
                let mut simple_enum_features = attributes
                    .parse_features::<EnumFeatures>()
                    .into_inner()
                    .unwrap_or_default();
                let schema_as = pop_feature_as_inner!(simple_enum_features => Feature::As(_v));
                let rename_all = simple_enum_features.pop_rename_all_feature();

                Self {
                    schema_type: EnumSchemaType::Simple(SimpleEnum {
                        attributes,
                        variants,
                        enum_features: simple_enum_features,
                        rename_all,
                    }),
                    schema_as,
                }
            }
        } else {
            let mut enum_features = attributes
                .parse_features::<ComplexEnumFeatures>()
                .into_inner()
                .unwrap_or_default();
            let schema_as = pop_feature_as_inner!(enum_features => Feature::As(_v));
            let rename_all = enum_features.pop_rename_all_feature();

            Self {
                schema_type: EnumSchemaType::Complex(ComplexEnum {
                    enum_name,
                    attributes,
                    variants,
                    rename_all,
                    enum_features,
                }),
                schema_as,
            }
        }
    }
}

impl ToTokens for EnumSchema<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.schema_type.to_tokens(tokens);
    }
}

#[derive(Debug)]
enum EnumSchemaType<'e> {
    Simple(SimpleEnum<'e>),
    #[cfg(feature = "repr")]
    Repr(ReprEnum<'e>),
    Complex(ComplexEnum<'e>),
}

impl ToTokens for EnumSchemaType<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let attributes = match self {
            Self::Simple(simple) => {
                simple.to_tokens(tokens);
                simple.attributes
            }
            #[cfg(feature = "repr")]
            Self::Repr(repr) => {
                repr.to_tokens(tokens);
                repr.attributes
            }
            Self::Complex(complex) => {
                complex.to_tokens(tokens);
                complex.attributes
            }
        };

        if let Some(deprecated) = super::get_deprecated(attributes) {
            tokens.extend(quote! { .deprecated(#deprecated) });
        }

        let description = CommentAttributes::from_attributes(attributes).as_formatted_string();
        if !description.is_empty() {
            tokens.extend(quote! {
                .description(#description)
            })
        }
    }
}

#[cfg(feature = "repr")]
#[derive(Debug)]
struct ReprEnum<'a> {
    variants: &'a Punctuated<Variant, Comma>,
    attributes: &'a [Attribute],
    enum_type: syn::TypePath,
    enum_features: Vec<Feature>,
}

#[cfg(feature = "repr")]
impl ToTokens for ReprEnum<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let container_rules = serde::parse_container(self.attributes);

        regular_enum_to_tokens(tokens, &container_rules, &self.enum_features, || {
            self.variants
                .iter()
                .filter_map(|variant| {
                    let variant_type = &variant.ident;
                    let variant_rules = serde::parse_value(&variant.attrs);

                    if is_not_skipped(&variant_rules) {
                        let repr_type = &self.enum_type;
                        Some(enum_variant::ReprVariant {
                            value: quote! { Self::#variant_type as #repr_type },
                            type_path: repr_type,
                        })
                    } else {
                        None
                    }
                })
                .collect::<Vec<enum_variant::ReprVariant<TokenStream>>>()
        });
    }
}

fn rename_enum_variant<'a>(
    name: &'a str,
    features: &mut Vec<Feature>,
    variant_rules: &'a Option<SerdeValue>,
    container_rules: &'a Option<SerdeContainer>,
    rename_all: &'a Option<RenameAll>,
) -> Option<Cow<'a, str>> {
    let rename = features.pop_rename_feature().map(|rename| rename.into_value());
    let rename_to = variant_rules
        .as_ref()
        .and_then(|variant_rules| variant_rules.rename.as_deref().map(Cow::Borrowed))
        .or_else(|| rename.map(Cow::Owned));

    let rename_all = container_rules
        .as_ref()
        .and_then(|container_rules| container_rules.rename_all.as_ref())
        .or_else(|| rename_all.as_ref().map(|rename_all| rename_all.as_rename_rule()));

    super::rename::<VariantRename>(name, rename_to, rename_all)
}

#[derive(Debug)]
struct SimpleEnum<'a> {
    variants: &'a Punctuated<Variant, Comma>,
    attributes: &'a [Attribute],
    enum_features: Vec<Feature>,
    rename_all: Option<RenameAll>,
}

impl ToTokens for SimpleEnum<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let container_rules = serde::parse_container(self.attributes);

        regular_enum_to_tokens(tokens, &container_rules, &self.enum_features, || {
            self.variants
                .iter()
                .filter_map(|variant| {
                    let variant_rules = serde::parse_value(&variant.attrs);

                    if is_not_skipped(&variant_rules) {
                        Some((variant, variant_rules))
                    } else {
                        None
                    }
                })
                .flat_map(|(variant, variant_rules)| {
                    let name = &*variant.ident.to_string();
                    let mut variant_features = features::parse_schema_features_with(&variant.attrs, |input| {
                        Ok(parse_features!(input as Rename))
                    })
                    .unwrap_or_default();
                    let variant_name = rename_enum_variant(
                        name,
                        &mut variant_features,
                        &variant_rules,
                        &container_rules,
                        &self.rename_all,
                    );

                    variant_name
                        .map(|name| SimpleEnumVariant {
                            value: name.to_token_stream(),
                        })
                        .or_else(|| {
                            Some(SimpleEnumVariant {
                                value: name.to_token_stream(),
                            })
                        })
                })
                .collect::<Vec<SimpleEnumVariant<TokenStream>>>()
        });
    }
}

fn regular_enum_to_tokens<T: self::enum_variant::Variant>(
    tokens: &mut TokenStream,
    container_rules: &Option<SerdeContainer>,
    enum_variant_features: &Vec<Feature>,
    get_variants_tokens_vec: impl FnOnce() -> Vec<T>,
) {
    let enum_values = get_variants_tokens_vec();

    tokens.extend(match container_rules {
        Some(serde_container) => match &serde_container.enum_repr {
            SerdeEnumRepr::ExternallyTagged => Enum::new(enum_values).to_token_stream(),
            SerdeEnumRepr::InternallyTagged { tag } => TaggedEnum::new(
                enum_values
                    .into_iter()
                    .map(|variant| (Cow::Borrowed(tag.as_str()), variant)),
            )
            .to_token_stream(),
            SerdeEnumRepr::Untagged => UntaggedEnum::new().to_token_stream(),
            SerdeEnumRepr::AdjacentlyTagged { tag, content } => AdjacentlyTaggedEnum::new(
                enum_values
                    .into_iter()
                    .map(|variant| (Cow::Borrowed(tag.as_str()), Cow::Borrowed(content.as_str()), variant)),
            )
            .to_token_stream(),
            // This should not be possible as serde should not let that happen
            SerdeEnumRepr::UnfinishedAdjacentlyTagged { .. } => panic!("Invalid serde enum repr"),
        },
        _ => Enum::new(enum_values).to_token_stream(),
    });

    tokens.extend(enum_variant_features.to_token_stream());
}

#[derive(Debug)]
struct ComplexEnum<'a> {
    variants: &'a Punctuated<Variant, Comma>,
    attributes: &'a [Attribute],
    enum_name: Cow<'a, str>,
    enum_features: Vec<Feature>,
    rename_all: Option<RenameAll>,
}

impl ComplexEnum<'_> {
    /// Produce tokens that represent a variant of a [`ComplexEnum`].
    fn variant_tokens(
        &self,
        name: Cow<'_, str>,
        variant: &Variant,
        variant_rules: &Option<SerdeValue>,
        container_rules: &Option<SerdeContainer>,
        rename_all: &Option<RenameAll>,
    ) -> TokenStream {
        // TODO need to be able to split variant.attrs for variant and the struct representation!
        match &variant.fields {
            Fields::Named(named_fields) => {
                let (title_features, mut named_struct_features) = variant
                    .attrs
                    .parse_features::<EnumNamedFieldVariantFeatures>()
                    .into_inner()
                    .map(|features| features.split_for_title())
                    .unwrap_or_default();
                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut named_struct_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let example = pop_feature!(named_struct_features => Feature::Example(_));

                self::enum_variant::Variant::to_tokens(&ObjectVariant {
                    name: variant_name.unwrap_or(Cow::Borrowed(&name)),
                    title: title_features.first().map(ToTokens::to_token_stream),
                    example: example.as_ref().map(ToTokens::to_token_stream),
                    item: NamedStructSchema {
                        struct_name: Cow::Borrowed(&*self.enum_name),
                        attributes: &variant.attrs,
                        rename_all: named_struct_features.pop_rename_all_feature(),
                        features: Some(named_struct_features),
                        fields: &named_fields.named,
                        generics: None,
                        aliases: None,
                        schema_as: None,
                    },
                })
            }
            Fields::Unnamed(unnamed_fields) => {
                let (title_features, mut unnamed_struct_features) = variant
                    .attrs
                    .parse_features::<EnumUnnamedFieldVariantFeatures>()
                    .into_inner()
                    .map(|features| features.split_for_title())
                    .unwrap_or_default();
                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut unnamed_struct_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let example = pop_feature!(unnamed_struct_features => Feature::Example(_));

                self::enum_variant::Variant::to_tokens(&ObjectVariant {
                    name: variant_name.unwrap_or(Cow::Borrowed(&name)),
                    title: title_features.first().map(ToTokens::to_token_stream),
                    example: example.as_ref().map(ToTokens::to_token_stream),
                    item: UnnamedStructSchema {
                        struct_name: Cow::Borrowed(&*self.enum_name),
                        attributes: &variant.attrs,
                        features: Some(unnamed_struct_features),
                        fields: &unnamed_fields.unnamed,
                        schema_as: None,
                    },
                })
            }
            Fields::Unit => {
                let mut unit_features = features::parse_schema_features_with(&variant.attrs, |input| {
                    Ok(parse_features!(
                        input as super::features::Title,
                        RenameAll,
                        Rename,
                        Example
                    ))
                })
                .unwrap_or_default();
                let title = pop_feature!(unit_features => Feature::Title(_));
                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut unit_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let example = pop_feature!(unit_features => Feature::Example(_));

                // Unit variant is just simple enum with single variant.
                let mut sev = Enum::new([SimpleEnumVariant {
                    value: variant_name.unwrap_or(Cow::Borrowed(&name)).to_token_stream(),
                }]);
                if let Some(title) = title {
                    sev = sev.with_title(title.to_token_stream());
                }
                if let Some(example) = example {
                    sev = sev.with_example(example.to_token_stream());
                }
                sev.to_token_stream()
            }
        }
    }

    /// Produce tokens that represent a variant of a [`ComplexEnum`] where serde enum attribute
    /// `untagged` applies.
    fn untagged_variant_tokens(&self, variant: &Variant) -> TokenStream {
        match &variant.fields {
            Fields::Named(named_fields) => {
                let mut named_struct_features = variant
                    .attrs
                    .parse_features::<EnumNamedFieldVariantFeatures>()
                    .into_inner()
                    .unwrap_or_default();

                NamedStructSchema {
                    struct_name: Cow::Borrowed(&*self.enum_name),
                    attributes: &variant.attrs,
                    rename_all: named_struct_features.pop_rename_all_feature(),
                    features: Some(named_struct_features),
                    fields: &named_fields.named,
                    generics: None,
                    aliases: None,
                    schema_as: None,
                }
                .to_token_stream()
            }
            Fields::Unnamed(unnamed_fields) => {
                let unnamed_struct_features = variant
                    .attrs
                    .parse_features::<EnumUnnamedFieldVariantFeatures>()
                    .into_inner()
                    .unwrap_or_default();

                UnnamedStructSchema {
                    struct_name: Cow::Borrowed(&*self.enum_name),
                    attributes: &variant.attrs,
                    features: Some(unnamed_struct_features),
                    fields: &unnamed_fields.unnamed,
                    schema_as: None,
                }
                .to_token_stream()
            }
            Fields::Unit => {
                let mut unit_features = features::parse_schema_features_with(&variant.attrs, |input| {
                    Ok(parse_features!(input as super::features::Title))
                })
                .unwrap_or_default();
                let title = pop_feature!(unit_features => Feature::Title(_));

                UntaggedEnum::with_title(title).to_token_stream()
            }
        }
    }

    /// Produce tokens that represent a variant of a [`ComplexEnum`] where serde enum attribute
    /// `tag = ` applies.
    fn tagged_variant_tokens(
        &self,
        tag: &str,
        name: Cow<'_, str>,
        variant: &Variant,
        variant_rules: &Option<SerdeValue>,
        container_rules: &Option<SerdeContainer>,
        rename_all: &Option<RenameAll>,
    ) -> TokenStream {
        let oapi = crate::oapi_crate();
        match &variant.fields {
            Fields::Named(named_fields) => {
                let (title_features, mut named_struct_features) = variant
                    .attrs
                    .parse_features::<EnumNamedFieldVariantFeatures>()
                    .into_inner()
                    .map(|features| features.split_for_title())
                    .unwrap_or_default();
                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut named_struct_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let named_enum = NamedStructSchema {
                    struct_name: Cow::Borrowed(&*self.enum_name),
                    attributes: &variant.attrs,
                    rename_all: named_struct_features.pop_rename_all_feature(),
                    features: Some(named_struct_features),
                    fields: &named_fields.named,
                    generics: None,
                    aliases: None,
                    schema_as: None,
                };
                let title = title_features.first().map(ToTokens::to_token_stream);

                let variant_name_tokens = Enum::new([SimpleEnumVariant {
                    value: variant_name.unwrap_or(Cow::Borrowed(&name)).to_token_stream(),
                }]);
                quote! {
                    #named_enum
                        #title
                        .property(#tag, #variant_name_tokens)
                        .required(#tag)
                }
            }
            Fields::Unnamed(unnamed_fields) => {
                if unnamed_fields.unnamed.len() == 1 {
                    let (title_features, mut unnamed_struct_features) = variant
                        .attrs
                        .parse_features::<EnumUnnamedFieldVariantFeatures>()
                        .into_inner()
                        .map(|features| features.split_for_title())
                        .unwrap_or_default();
                    let variant_name = rename_enum_variant(
                        name.as_ref(),
                        &mut unnamed_struct_features,
                        variant_rules,
                        container_rules,
                        rename_all,
                    );

                    let unnamed_enum = UnnamedStructSchema {
                        struct_name: Cow::Borrowed(&*self.enum_name),
                        attributes: &variant.attrs,
                        features: Some(unnamed_struct_features),
                        fields: &unnamed_fields.unnamed,
                        schema_as: None,
                    };

                    let title = title_features.first().map(ToTokens::to_token_stream);
                    let variant_name_tokens = Enum::new([SimpleEnumVariant {
                        value: variant_name.unwrap_or(Cow::Borrowed(&name)).to_token_stream(),
                    }]);

                    let is_reference = unnamed_fields.unnamed.iter().any(|field| {
                        let ty = TypeTree::from_type(&field.ty);

                        ty.value_type == ValueType::Object
                    });

                    if is_reference {
                        quote! {
                            #oapi::oapi::schema::AllOf::new()
                                #title
                                .item(#unnamed_enum)
                                .item(#oapi::oapi::schema::Object::new()
                                    .schema_type(#oapi::oapi::schema::SchemaType::Object)
                                    .property(#tag, #variant_name_tokens)
                                    .required(#tag)
                                )
                        }
                    } else {
                        quote! {
                            #unnamed_enum
                                #title
                                .schema_type(#oapi::oapi::schema::SchemaType::Object)
                                .property(#tag, #variant_name_tokens)
                                .required(#tag)
                        }
                    }
                } else {
                    abort!(
                        variant,
                        "Unnamed (tuple) enum variants are unsupported for internally tagged enums using the `tag = ` serde attribute";

                        help = "Try using a different serde enum representation";
                        note = "See more about enum limitations here: `https://serde.rs/enum-representations.html#internally-tagged`"
                    );
                }
            }
            Fields::Unit => {
                let mut unit_features = features::parse_schema_features_with(&variant.attrs, |input| {
                    Ok(parse_features!(input as super::features::Title, Rename))
                })
                .unwrap_or_default();
                let title = pop_feature!(unit_features => Feature::Title(_));

                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut unit_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                // Unit variant is just simple enum with single variant.
                let variant_tokens = Enum::new([SimpleEnumVariant {
                    value: variant_name.unwrap_or(Cow::Borrowed(&name)).to_token_stream(),
                }]);

                quote! {
                    #oapi::oapi::schema::Object::new()
                        #title
                        .property(#tag, #variant_tokens)
                        .required(#tag)
                }
            }
        }
    }

    // FIXME perhaps design this better to lessen the amount of args.
    #[allow(clippy::too_many_arguments)]
    fn adjacently_tagged_variant_tokens(
        &self,
        tag: &str,
        content: &str,
        name: Cow<'_, str>,
        variant: &Variant,
        variant_rules: &Option<SerdeValue>,
        container_rules: &Option<SerdeContainer>,
        rename_all: &Option<RenameAll>,
    ) -> TokenStream {
        let oapi = crate::oapi_crate();
        match &variant.fields {
            Fields::Named(named_fields) => {
                let (title_features, mut named_struct_features) = variant
                    .attrs
                    .parse_features::<EnumNamedFieldVariantFeatures>()
                    .into_inner()
                    .map(|features| features.split_for_title())
                    .unwrap_or_default();
                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut named_struct_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let named_enum = NamedStructSchema {
                    struct_name: Cow::Borrowed(&*self.enum_name),
                    attributes: &variant.attrs,
                    rename_all: named_struct_features.pop_rename_all_feature(),
                    features: Some(named_struct_features),
                    fields: &named_fields.named,
                    generics: None,
                    aliases: None,
                    schema_as: None,
                };
                let title = title_features.first().map(ToTokens::to_token_stream);

                let variant_name_tokens = Enum::new([SimpleEnumVariant {
                    value: variant_name.unwrap_or(Cow::Borrowed(&name)).to_token_stream(),
                }]);
                quote! {
                    #oapi::oapi::schema::Object::new()
                        #title
                        .schema_type(#oapi::oapi::schema::SchemaType::Object)
                        .property(#tag, #variant_name_tokens)
                        .required(#tag)
                        .property(#content, #named_enum)
                        .required(#content)
                }
            }
            Fields::Unnamed(unnamed_fields) => {
                if unnamed_fields.unnamed.len() == 1 {
                    let (title_features, mut unnamed_struct_features) = variant
                        .attrs
                        .parse_features::<EnumUnnamedFieldVariantFeatures>()
                        .into_inner()
                        .map(|features| features.split_for_title())
                        .unwrap_or_default();
                    let variant_name = rename_enum_variant(
                        name.as_ref(),
                        &mut unnamed_struct_features,
                        variant_rules,
                        container_rules,
                        rename_all,
                    );

                    let unnamed_enum = UnnamedStructSchema {
                        struct_name: Cow::Borrowed(&*self.enum_name),
                        attributes: &variant.attrs,
                        features: Some(unnamed_struct_features),
                        fields: &unnamed_fields.unnamed,
                        schema_as: None,
                    };

                    let title = title_features.first().map(ToTokens::to_token_stream);
                    let variant_name_tokens = Enum::new([SimpleEnumVariant {
                        value: variant_name.unwrap_or(Cow::Borrowed(&name)).to_token_stream(),
                    }]);

                    quote! {
                        #oapi::oapi::schema::Object::new()
                            #title
                            .schema_type(#oapi::oapi::schema::SchemaType::Object)
                            .property(#tag, #variant_name_tokens)
                            .required(#tag)
                            .property(#content, #unnamed_enum)
                            .required(#content)
                    }
                } else {
                    abort!(
                        variant,
                        "Unnamed (tuple) enum variants are unsupported for adjacently tagged enums using the `tag = <tag>, content = <content>` serde attribute";

                        help = "Try using a different serde enum representation";
                        note = "See more about enum limitations here: `https://serde.rs/enum-representations.html#adjacently-tagged`"
                    );
                }
            }
            Fields::Unit => {
                // In this case `content` is simply ignored - there is nothing to put in it.

                let mut unit_features = features::parse_schema_features_with(&variant.attrs, |input| {
                    Ok(parse_features!(input as super::features::Title, Rename))
                })
                .unwrap_or_default();
                let title = pop_feature!(unit_features => Feature::Title(_));

                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut unit_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                // Unit variant is just simple enum with single variant.
                let variant_tokens = Enum::new([SimpleEnumVariant {
                    value: variant_name.unwrap_or(Cow::Borrowed(&name)).to_token_stream(),
                }]);

                quote! {
                    #oapi::oapi::schema::Object::new()
                        #title
                        .property(#tag, #variant_tokens)
                        .required(#tag)
                }
            }
        }
    }
}

impl ToTokens for ComplexEnum<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let attributes = &self.attributes;
        let container_rules = serde::parse_container(attributes);

        let enum_repr = container_rules
            .as_ref()
            .map(|rules| rules.enum_repr.clone())
            .unwrap_or_default();
        let tag = match &enum_repr {
            SerdeEnumRepr::AdjacentlyTagged { tag, .. } | SerdeEnumRepr::InternallyTagged { tag } => Some(tag),
            SerdeEnumRepr::ExternallyTagged
            | SerdeEnumRepr::Untagged
            | SerdeEnumRepr::UnfinishedAdjacentlyTagged { .. } => None,
        };

        let ts = self
            .variants
            .iter()
            .filter_map(|variant: &Variant| {
                let variant_serde_rules = serde::parse_value(&variant.attrs);
                if is_not_skipped(&variant_serde_rules) {
                    Some((variant, variant_serde_rules))
                } else {
                    None
                }
            })
            .map(|(variant, variant_serde_rules)| {
                let variant_name = &*variant.ident.to_string();

                match &enum_repr {
                    SerdeEnumRepr::ExternallyTagged => self.variant_tokens(
                        Cow::Borrowed(variant_name),
                        variant,
                        &variant_serde_rules,
                        &container_rules,
                        &self.rename_all,
                    ),
                    SerdeEnumRepr::InternallyTagged { tag } => self.tagged_variant_tokens(
                        tag,
                        Cow::Borrowed(variant_name),
                        variant,
                        &variant_serde_rules,
                        &container_rules,
                        &self.rename_all,
                    ),
                    SerdeEnumRepr::Untagged => self.untagged_variant_tokens(variant),
                    SerdeEnumRepr::AdjacentlyTagged { tag, content } => self.adjacently_tagged_variant_tokens(
                        tag,
                        content,
                        Cow::Borrowed(variant_name),
                        variant,
                        &variant_serde_rules,
                        &container_rules,
                        &self.rename_all,
                    ),
                    SerdeEnumRepr::UnfinishedAdjacentlyTagged { .. } => {
                        unreachable!("Serde should not have parsed an UnfinishedAdjacentlyTagged")
                    }
                }
            })
            .collect::<CustomEnum<'_, TokenStream>>();
        if let Some(tag) = tag {
            ts.with_discriminator(Cow::Borrowed(tag.as_str())).to_tokens(tokens);
        } else {
            ts.to_tokens(tokens);
        }

        tokens.extend(self.enum_features.to_token_stream());
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

/// Reformat a path reference string that was generated using [`quote`] to be used as a nice compact schema reference,
/// by removing spaces between colon punctuation and `::` and the path segments.
pub(crate) fn format_path_ref(path: &Path) -> String {
    let mut path = path.clone();

    // Generics and path arguments are unsupported
    if let Some(last_segment) = path.segments.last_mut() {
        last_segment.arguments = PathArguments::None;
    }
    // :: are not officially supported in the spec
    // See: https://github.com/juhaku/salvo_oapi/pull/187#issuecomment-1173101405
    path.to_token_stream().to_string().replace(" :: ", ".")
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
pub struct AliasSchema {
    pub name: String,
    pub ty: Type,
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
