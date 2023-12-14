use std::borrow::Cow;

use proc_macro2::TokenStream;
use proc_macro_error::abort;
use quote::{format_ident, quote, ToTokens};
use syn::{punctuated::Punctuated, spanned::Spanned, Attribute, Field, Generics, Token};

use crate::{
    component::ComponentSchemaProps,
    doc_comment::CommentAttributes,
    feature::{pop_feature, pop_feature_as_inner, Feature, FeaturesExt, IntoInner, RenameAll, Symbol, ToTokensExt},
    schema::Inline,
    type_tree::TypeTree,
    Deprecated,
};

use super::{
    feature::{FromAttributes, NamedFieldFeatures},
    is_flatten, is_not_skipped,
    serde::{self, SerdeContainer},
    ComponentSchema, FieldRename, FlattenedMapSchema, Property,
};

#[derive(Debug)]
pub(crate) struct NamedStructSchema<'a> {
    pub(crate) struct_name: Cow<'a, str>,
    pub(crate) fields: &'a Punctuated<Field, Token![,]>,
    pub(crate) attributes: &'a [Attribute],
    pub(crate) features: Option<Vec<Feature>>,
    pub(crate) rename_all: Option<RenameAll>,
    #[allow(dead_code)]
    pub(crate) generics: Option<&'a Generics>,
    pub(crate) symbol: Option<Symbol>,
    pub(crate) inline: Option<Inline>,
}

struct NamedStructFieldOptions<'a> {
    property: Property,
    rename_field_value: Option<Cow<'a, str>>,
    required: Option<crate::feature::Required>,
    is_option: bool,
}

impl NamedStructSchema<'_> {
    fn field_as_schema_property<R>(
        &self,
        field: &Field,
        flatten: bool,
        container_rules: &Option<SerdeContainer>,
        yield_: impl FnOnce(NamedStructFieldOptions<'_>) -> R,
    ) -> R {
        let type_tree = &mut TypeTree::from_type(&field.ty);
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
                features_inner.push(Feature::Default(crate::feature::Default::new_default_trait(
                    struct_ident,
                    field_ident.into(),
                )));
            }
        }

        let rename_field = pop_feature!(field_features => Feature::Rename(_)).and_then(|feature| match feature {
            Feature::Rename(rename) => Some(Cow::Owned(rename.into_value())),
            _ => None,
        });

        let deprecated = crate::get_deprecated(&field.attrs).or_else(|| {
            pop_feature!(field_features => Feature::Deprecated(_)).and_then(|feature| match feature {
                Feature::Deprecated(_) => Some(Deprecated::True),
                _ => None,
            })
        });

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
                Property::SchemaWith(with_schema)
            } else {
                let cs = ComponentSchemaProps {
                    type_tree,
                    features: field_features,
                    description: Some(&comments),
                    deprecated: deprecated.as_ref(),
                    object_name: self.struct_name.as_ref(),
                    type_definition: true,
                };
                if flatten && type_tree.is_map() {
                    Property::FlattenedMap(FlattenedMapSchema::new(cs))
                } else {
                    Property::Schema(ComponentSchema::new(cs))
                }
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

        let mut object_tokens = self
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
                        false,
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

                            let name = crate::rename::<FieldRename>(field_name, rename_to, rename_all)
                                .unwrap_or(Cow::Borrowed(field_name));

                            object_tokens.extend(quote! {
                                .property(#name, #property)
                            });

                            if (!is_option && crate::is_required(field_rule.as_ref(), container_rules.as_ref()))
                                || required
                                    .as_ref()
                                    .map(crate::feature::Required::is_true)
                                    .unwrap_or(false)
                            {
                                object_tokens.extend(quote! {
                                    .required(#name)
                                })
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

        let all_of = if !flatten_fields.is_empty() {
            let mut flattened_tokens = TokenStream::new();
            let mut flattened_map_field = None;

            for field in flatten_fields {
                self.field_as_schema_property(
                    field,
                    true,
                    &container_rules,
                    |NamedStructFieldOptions { property, .. }| match property {
                        Property::Schema(_) | Property::SchemaWith(_) => {
                            flattened_tokens.extend(quote! { .item(#property) })
                        }
                        Property::FlattenedMap(_) => match flattened_map_field {
                            None => {
                                object_tokens
                                    .extend(quote! { .additional_properties(Some(#property)) });
                                flattened_map_field = Some(field);
                            }
                            Some(flattened_map_field) => {
                                abort!(self.fields,
                                       "The structure `{}` contains multiple flattened map fields.",
                                                                             self.struct_name;
                                       note = flattened_map_field.span() => "first flattened map field was declared here as `{}`", flattened_map_field.ident.as_ref().unwrap();
                                       note = field.span() => "second flattened map field was declared here as `{}`", field.ident.as_ref().unwrap());
                            },
                        },
                    },
                )
            }

            if flattened_tokens.is_empty() {
                tokens.extend(object_tokens);
                false
            } else {
                tokens.extend(quote! {
                    utoipa::openapi::AllOfBuilder::new()
                        #flattened_tokens
                    .item(#object_tokens)
                });

                true
            }
        } else {
            tokens.extend(object_tokens);
            false
        };

        if !all_of
            && container_rules
                .as_ref()
                .map(|container_rule| container_rule.deny_unknown_fields)
                .unwrap_or(false)
        {
            tokens.extend(quote! {
                .additional_properties(Some(utoipa::openapi::schema::AdditionalProperties::FreeForm(false)))
            });
        }

        if let Some(deprecated) = crate::get_deprecated(self.attributes) {
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
pub(super) struct UnnamedStructSchema<'a> {
    pub(super) struct_name: Cow<'a, str>,
    pub(super) fields: &'a Punctuated<Field, Token![,]>,
    pub(super) attributes: &'a [Attribute],
    pub(super) features: Option<Vec<Feature>>,
    pub(super) symbol: Option<Symbol>,
    pub(super) inline: Option<Inline>,
}

impl ToTokens for UnnamedStructSchema<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        let fields_len = self.fields.len();
        let first_field = self.fields.first().unwrap();
        let first_part = &TypeTree::from_type(&first_field.ty);

        let all_fields_are_same = fields_len == 1
            || self.fields.iter().skip(1).all(|field| {
                let schema_part = &TypeTree::from_type(&field.ty);

                first_part == schema_part
            });

        let deprecated = crate::get_deprecated(self.attributes);
        if all_fields_are_same {
            let mut unnamed_struct_features = self.features.clone();
            let value_type = unnamed_struct_features
                .as_mut()
                .and_then(|features| features.pop_value_type_feature());
            let override_type_tree = value_type.as_ref().map(|value_type| value_type.as_type_tree());

            if fields_len == 1 {
                if let Some(ref mut features) = unnamed_struct_features {
                    if pop_feature!(features => Feature::Default(crate::feature::Default(None))).is_some() {
                        let struct_ident = format_ident!("{}", &self.struct_name);
                        let index: syn::Index = 0.into();
                        features.push(Feature::Default(crate::feature::Default::new_default_trait(
                            struct_ident,
                            index.into(),
                        )));
                    }
                }
            }

            tokens.extend(
                ComponentSchema::new(ComponentSchemaProps {
                    type_tree: override_type_tree.as_ref().unwrap_or(first_part),
                    features: unnamed_struct_features,
                    description: Some(&CommentAttributes::from_attributes(self.attributes)),
                    deprecated: deprecated.as_ref(),
                    object_name: self.struct_name.as_ref(),
                    type_definition: true,
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

        if fields_len > 1 {
            let description = CommentAttributes::from_attributes(self.attributes).as_formatted_string();
            tokens.extend(
                quote! { .to_array_builder().description(Some(#description)).max_items(Some(#fields_len)).min_items(Some(#fields_len)) },
            )
        }
    }
}
