use std::borrow::Cow;

use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::token::Comma;
use syn::{Attribute, Field, Generics, Token, punctuated::Punctuated, spanned::Spanned};

use crate::component::{ComponentDescription, ComponentSchemaProps};
use crate::doc_comment::CommentAttributes;
use crate::feature::attributes::{
    self, Alias, Bound, Default, Name, RenameAll, Required, SkipBound,
};
use crate::feature::{
    Feature, FeaturesExt, IsSkipped, TryToTokensExt, parse_features, pop_feature,
    pop_feature_as_inner,
};
use crate::schema::{Description, Inline};
use crate::type_tree::TypeTree;
use crate::{
    Deprecated, DiagLevel, DiagResult, Diagnostic, IntoInner, SerdeContainer, SerdeValue,
    TryToTokens, serde_util,
};

use super::{
    ComponentSchema, FieldRename, FlattenedMapSchema, Property,
    feature::{FromAttributes, NamedFieldFeatures, parse_schema_features_with},
    is_flatten, is_not_skipped,
};

#[derive(Debug)]
pub(crate) struct NamedStructSchema<'a> {
    pub(crate) struct_name: Cow<'a, str>,
    pub(crate) fields: &'a Punctuated<Field, Token![,]>,
    pub(crate) attributes: &'a [Attribute],
    pub(crate) description: Option<Description>,
    pub(crate) features: Option<Vec<Feature>>,
    pub(crate) rename_all: Option<RenameAll>,
    #[allow(dead_code)]
    pub(crate) generics: Option<&'a Generics>,
    pub(crate) name: Option<Name>,
    pub(crate) aliases: Option<Punctuated<Alias, Token![,]>>,
    pub(crate) inline: Option<Inline>,
}

struct NamedStructFieldOptions<'a> {
    property: Property,
    rename_field_value: Option<Cow<'a, str>>,
    required: Option<Required>,
    is_option: bool,
}

impl NamedStructSchema<'_> {
    pub(crate) fn pop_skip_bound(&mut self) -> Option<SkipBound> {
        pop_feature_as_inner!(self.features => Feature::SkipBound(_v))
    }
    pub(crate) fn pop_bound(&mut self) -> Option<Bound> {
        pop_feature_as_inner!(self.features => Feature::Bound(_v))
    }
    fn field_as_schema_property(
        &self,
        field: &Field,
        flatten: bool,
        container_rules: &Option<SerdeContainer>,
    ) -> DiagResult<NamedStructFieldOptions<'_>> {
        let type_tree = &mut TypeTree::from_type(&field.ty)?;

        let mut field_features = field
            .attrs
            .parse_features::<NamedFieldFeatures>()?
            .into_inner();

        let schema_default = self
            .features
            .as_ref()
            .map(|features| features.iter().any(|f| matches!(f, Feature::Default(_))))
            .unwrap_or(false);
        let serde_default = container_rules
            .as_ref()
            .map(|rules| rules.is_default)
            .unwrap_or(false);

        if schema_default || serde_default {
            let features_inner = field_features.get_or_insert(vec![]);
            if !features_inner
                .iter()
                .any(|f| matches!(f, Feature::Default(_)))
            {
                let field_ident = field
                    .ident
                    .as_ref()
                    .expect("field ident shoule be exist")
                    .to_owned();
                let struct_ident = format_ident!("{}", &self.struct_name);
                features_inner.push(Feature::Default(Default::new_default_trait(
                    struct_ident,
                    field_ident.into(),
                )));
            }
        }

        let rename_field =
            pop_feature!(field_features => Feature::Rename(_)).and_then(|feature| match feature {
                Feature::Rename(rename) => Some(Cow::Owned(rename.into_value())),
                _ => None,
            });

        let deprecated = crate::get_deprecated(&field.attrs).or_else(|| {
            pop_feature!(field_features => Feature::Deprecated(_)).and_then(|feature| match feature
            {
                Feature::Deprecated(_) => Some(Deprecated::True),
                _ => None,
            })
        });

        let value_type = field_features
            .as_mut()
            .and_then(|features| features.pop_value_type_feature());
        let override_type_tree = value_type
            .as_ref()
            .map(|value_type| value_type.as_type_tree())
            .transpose()?;
        let comments = CommentAttributes::from_attributes(&field.attrs);
        let description = &ComponentDescription::CommentAttributes(&comments);
        let with_schema = pop_feature!(field_features => Feature::SchemaWith(_));
        let required = pop_feature_as_inner!(field_features => Feature::Required(_v));
        let type_tree = override_type_tree.as_ref().unwrap_or(type_tree);
        let is_option = type_tree.is_option();

        Ok(NamedStructFieldOptions {
            property: if let Some(with_schema) = with_schema {
                Property::SchemaWith(with_schema)
            } else {
                let cs = ComponentSchemaProps {
                    type_tree,
                    features: field_features,
                    description: Some(description),
                    deprecated: deprecated.as_ref(),
                    object_name: self.struct_name.as_ref(),
                };
                if flatten && type_tree.is_map() {
                    Property::FlattenedMap(FlattenedMapSchema::new(cs)?)
                } else {
                    Property::Schema(ComponentSchema::new(cs)?)
                }
            },
            rename_field_value: rename_field,
            required,
            is_option,
        })
    }
}

impl TryToTokens for NamedStructSchema<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let container_rules = serde_util::parse_container(self.attributes);

        let field_values = self
            .fields
            .iter()
            .map(|field| {
                let is_skipped = field
                    .attrs
                    .parse_features::<NamedFieldFeatures>()?
                    .into_inner()
                    .map(|features| features.is_skipped())
                    .unwrap_or(false);

                if is_skipped {
                    return Ok(None);
                }

                let field_rule = serde_util::parse_value(&field.attrs);

                if is_not_skipped(field_rule.as_ref()) && !is_flatten(field_rule.as_ref()) {
                    Ok(Some((field, field_rule)))
                } else {
                    Ok(None)
                }
            })
            .collect::<DiagResult<Vec<Option<(&Field, Option<SerdeValue>)>>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        let mut object_tokens = quote! { #oapi::oapi::Object::new() };
        for (field, field_rule) in field_values {
            let mut field_name = &*field
                .ident
                .as_ref()
                .expect("field ident shoule be exists")
                .to_string();

            if field_name.starts_with("r#") {
                field_name = &field_name[2..];
            }

            let NamedStructFieldOptions {
                property,
                rename_field_value,
                required,
                is_option,
            } = self.field_as_schema_property(field, false, &container_rules)?;
            let rename_to = field_rule
                .as_ref()
                .and_then(|field_rule| field_rule.rename.as_deref().map(Cow::Borrowed))
                .or(rename_field_value);
            let rename_all = container_rules
                .as_ref()
                .and_then(|container_rule| container_rule.rename_all.as_ref())
                .or_else(|| {
                    self.rename_all
                        .as_ref()
                        .map(|rename_all| rename_all.as_rename_rule())
                });

            let name = crate::rename::<FieldRename>(field_name, rename_to, rename_all)
                .unwrap_or(Cow::Borrowed(field_name));

            let property = property.try_to_token_stream()?;
            object_tokens.extend(quote! {
                .property(#name, #property)
            });

            let component_required =
                !is_option && crate::is_required(field_rule.as_ref(), container_rules.as_ref());
            let required = match (required, component_required) {
                (Some(required), _) => required.is_true(),
                (None, component_required) => component_required,
            };

            if required {
                object_tokens.extend(quote! {
                    .required(#name)
                })
            }
        }

        let flatten_fields: Vec<&Field> = self
            .fields
            .iter()
            .filter(|field| {
                let field_rule = serde_util::parse_value(&field.attrs);
                is_flatten(field_rule.as_ref())
            })
            .collect();

        let all_of = if !flatten_fields.is_empty() {
            let mut flattened_tokens = TokenStream::new();
            let mut flattened_map_field = None;

            for field in flatten_fields {
                let NamedStructFieldOptions { property, .. } =
                    self.field_as_schema_property(field, true, &container_rules)?;

                match property {
                    Property::Schema(_) | Property::SchemaWith(_) => {
                        let property = property.try_to_token_stream()?;
                        flattened_tokens.extend(quote! { .item(#property) })
                    }
                    Property::FlattenedMap(_) => match flattened_map_field {
                        None => {
                            let property = property.try_to_token_stream()?;
                            object_tokens
                                .extend(quote! { .additional_properties(Some(#property)) });
                            flattened_map_field = Some(field);
                        }
                        Some(flattened_map_field) => {
                            return Err(Diagnostic::spanned(
                                self.fields.span(),
                                DiagLevel::Error,
                                format!(
                                    "The structure `{}` contains multiple flattened map fields.",
                                    self.struct_name
                                ),
                            )
                            .note(format!(
                                "first flattened map field was declared here as `{:?}`",
                                flattened_map_field.ident
                            ))
                            .note(format!(
                                "second flattened map field was declared here as `{:?}`",
                                field.ident
                            )));
                        }
                    },
                }
            }

            if flattened_tokens.is_empty() {
                tokens.extend(object_tokens);
                false
            } else {
                tokens.extend(quote! {
                    #oapi::oapi::schema::AllOf::new()
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
                .additional_properties(#oapi::oapi::schema::AdditionalProperties::FreeForm(false))
            });
        }

        if let Some(deprecated) = crate::get_deprecated(self.attributes) {
            tokens.extend(quote! { .deprecated(#deprecated) });
        }

        if let Some(struct_features) = self.features.as_ref() {
            tokens.extend(struct_features.try_to_token_stream()?)
        }

        let comments = CommentAttributes::from_attributes(self.attributes);
        let description = self
            .description
            .as_ref()
            .map(ComponentDescription::Description)
            .or(Some(ComponentDescription::CommentAttributes(&comments)));

        description.to_tokens(tokens);

        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct UnnamedStructSchema<'a> {
    pub(super) struct_name: Cow<'a, str>,
    pub(super) fields: &'a Punctuated<Field, Token![,]>,
    pub(super) description: Option<Description>,
    pub(super) attributes: &'a [Attribute],
    pub(super) features: Option<Vec<Feature>>,
    pub(super) name: Option<Name>,
    pub(super) aliases: Option<Punctuated<Alias, Comma>>,
    pub(super) inline: Option<Inline>,
}
impl UnnamedStructSchema<'_> {
    pub(crate) fn pop_skip_bound(&mut self) -> Option<SkipBound> {
        pop_feature_as_inner!(self.features => Feature::SkipBound(_v))
    }
    pub(crate) fn pop_bound(&mut self) -> Option<Bound> {
        pop_feature_as_inner!(self.features => Feature::Bound(_v))
    }
}

impl TryToTokens for UnnamedStructSchema<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let fields_len = self.fields.len();
        let first_field = self.fields.first().expect("fields should not be empty");
        let first_part = &TypeTree::from_type(&first_field.ty)?;

        let all_fields_are_same = fields_len == 1
            || self
                .fields
                .iter()
                .skip(1)
                .map(|field| TypeTree::from_type(&field.ty))
                .collect::<Result<Vec<TypeTree>, Diagnostic>>()?
                .iter()
                .all(|schema_part| first_part == schema_part);

        let deprecated = crate::get_deprecated(self.attributes);
        if all_fields_are_same {
            let mut unnamed_struct_features = self.features.clone();
            let value_type = unnamed_struct_features
                .as_mut()
                .and_then(|features| features.pop_value_type_feature());
            let override_type_tree = value_type
                .as_ref()
                .map(|value_type| value_type.as_type_tree())
                .transpose()?;

            if fields_len == 1 {
                if let Some(ref mut features) = unnamed_struct_features {
                    let inline = parse_schema_features_with(&first_field.attrs, |input| {
                        Ok(parse_features!(input as attributes::Inline))
                    })?
                    .unwrap_or_default();

                    features.extend(inline);

                    if pop_feature!(features => Feature::Default(Default(None))).is_some() {
                        let struct_ident = format_ident!("{}", &self.struct_name);
                        let index: syn::Index = 0.into();
                        features.push(Feature::Default(Default::new_default_trait(
                            struct_ident,
                            index.into(),
                        )));
                    }
                }
            }
            let comments = CommentAttributes::from_attributes(self.attributes);
            let description = self
                .description
                .as_ref()
                .map(ComponentDescription::Description)
                .or(Some(ComponentDescription::CommentAttributes(&comments)));

            tokens.extend(
                ComponentSchema::new(ComponentSchemaProps {
                    type_tree: override_type_tree.as_ref().unwrap_or(first_part),
                    features: unnamed_struct_features,
                    description: description.as_ref(),
                    deprecated: deprecated.as_ref(),
                    object_name: self.struct_name.as_ref(),
                })?
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
                let attrs = attrs
                    .iter()
                    .map(TryToTokens::try_to_token_stream)
                    .collect::<DiagResult<TokenStream>>()?;
                tokens.extend(attrs)
            }
        };

        if fields_len > 1 {
            let comments = CommentAttributes::from_attributes(self.attributes);
            let description = self
                .description
                .as_ref()
                .map(ComponentDescription::Description)
                .or(Some(ComponentDescription::CommentAttributes(&comments)));
            tokens.extend(quote! {
            .to_array_builder()
                .max_items(Some(#fields_len))
                .min_items(Some(#fields_len))
                #description
            })
        }
        Ok(())
    }
}
