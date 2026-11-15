use std::borrow::Cow;

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::{Attribute, Field, Generics, Token};

use super::feature::{FromAttributes, NamedFieldFeatures, parse_schema_features_with};
use super::{
    ComponentSchema, FieldRename, FlattenedMapSchema, Property, is_flatten, is_not_skipped,
};
use crate::component::{ComponentDescription, ComponentSchemaProps};
use crate::doc_comment::CommentAttributes;
use crate::feature::attributes::{
    self, Alias, Bound, Default, Ignore, Name, NoRecursion, RenameAll, Required, SkipBound,
};
use crate::feature::{
    Feature, FeaturesExt, IsSkipped, parse_features, pop_feature, pop_feature_as_inner,
};
use crate::schema::{Description, Inline};
use crate::type_tree::TypeTree;
use crate::{
    AnyValue, Deprecated, DiagLevel, DiagResult, Diagnostic, IntoInner, SerdeContainer, SerdeValue,
    TryToTokens, serde_util,
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
    pub(crate) compose_context: Option<crate::component::ComposeContext>,
}

struct NamedStructFieldOptions<'a> {
    property: Property,
    rename_field_value: Option<Cow<'a, str>>,
    required: Option<Required>,
    is_option: bool,
    ignore: Option<crate::parse_utils::LitBoolOrExprPath>,
}

pub(crate) fn transparent_field(
    fields: &Punctuated<Field, Token![,]>,
) -> DiagResult<(usize, &Field)> {
    let mut selected = None;
    for (index, field) in fields.iter().enumerate() {
        let rules = serde_util::parse_value(&field.attrs).map_err(Diagnostic::from)?;
        if is_not_skipped(rules.as_ref()) {
            if selected.is_some() {
                return Err(Diagnostic::spanned(
                    fields.span(),
                    DiagLevel::Error,
                    "serde transparent requires exactly one serialized field",
                ));
            }
            selected = Some((index, field));
        }
    }
    selected.ok_or_else(|| {
        Diagnostic::spanned(
            fields.span(),
            DiagLevel::Error,
            "serde transparent requires a serialized field",
        )
    })
}

fn transparent_schema(
    fields: &Punctuated<Field, Token![,]>,
    attributes: &[Attribute],
    mut features: Option<Vec<Feature>>,
    description: Option<&Description>,
    struct_name: &str,
    compose_context: Option<&crate::component::ComposeContext>,
) -> DiagResult<TokenStream> {
    let (index, field) = transparent_field(fields)?;
    let field_features = field
        .attrs
        .parse_features::<NamedFieldFeatures>()?
        .into_inner()
        .unwrap_or_default();
    let features = features.get_or_insert_with(Vec::new);
    for feature in field_features {
        // Property names, requiredness and visibility do not wrap the value of
        // a transparent container. All value-level features retain their meaning.
        if matches!(
            feature,
            Feature::Rename(_) | Feature::Required(_) | Feature::Skip(_) | Feature::Ignore(_)
        ) {
            continue;
        }
        features.retain(|existing| {
            std::mem::discriminant(existing) != std::mem::discriminant(&feature)
        });
        features.push(feature);
    }
    if let Some(with_schema) = pop_feature!(features => Feature::SchemaWith(_)) {
        return with_schema.try_to_token_stream();
    }
    let value_type = features.pop_value_type_feature();
    let type_tree = value_type
        .as_ref()
        .map(|value| value.as_type_tree())
        .transpose()?
        .unwrap_or(TypeTree::from_type(&field.ty)?);
    let member = field
        .ident
        .clone()
        .map(syn::Member::Named)
        .unwrap_or_else(|| syn::Member::Unnamed(index.into()));
    if pop_feature!(features => Feature::Default(Default(None))).is_some() {
        features.push(Feature::Default(Default::new_default_trait(member)));
    } else if !features
        .iter()
        .any(|feature| matches!(feature, Feature::Default(_)))
        && let Some(rules) = serde_util::parse_container(attributes).map_err(Diagnostic::from)?
        && rules.is_default
    {
        let constructor = match rules.default_path {
            Some(path) => quote! { #path() },
            None => quote! { Self::default() },
        };
        features.push(Feature::Default(Default(Some(AnyValue::Json(quote! {
            (#constructor).#member
        })))));
    }
    let comments = CommentAttributes::from_attributes(attributes);
    let field_comments = CommentAttributes::from_attributes(&field.attrs);
    let description = description
        .map(ComponentDescription::Description)
        .unwrap_or(ComponentDescription::CommentAttributes(
            if comments.is_empty() {
                &field_comments
            } else {
                &comments
            },
        ));
    let field_deprecated =
        pop_feature!(features => Feature::Deprecated(_)).and_then(|feature| match feature {
            Feature::Deprecated(deprecated) => Some(Deprecated::from(deprecated.0)),
            _ => None,
        });
    let deprecated = crate::get_deprecated(attributes)
        .or_else(|| crate::get_deprecated(&field.attrs))
        .or(field_deprecated);
    Ok(ComponentSchema::new(ComponentSchemaProps {
        type_tree: &type_tree,
        features: Some(features.clone()),
        description: Some(&description),
        deprecated: deprecated.as_ref(),
        object_name: struct_name,
        compose_context,
    })?
    .to_token_stream())
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
        container_rules: Option<&SerdeContainer>,
    ) -> DiagResult<NamedStructFieldOptions<'_>> {
        let type_tree = &mut TypeTree::from_type(&field.ty)?;

        let mut field_features = field
            .attrs
            .parse_features::<NamedFieldFeatures>()?
            .into_inner();

        // Propagate no_recursion from struct-level features to field-level features
        if self
            .features
            .as_ref()
            .map(|features| {
                features
                    .iter()
                    .any(|f| matches!(f, Feature::NoRecursion(_)))
            })
            .unwrap_or(false)
        {
            let features_inner = field_features.get_or_insert(vec![]);
            if !features_inner
                .iter()
                .any(|f| matches!(f, Feature::NoRecursion(_)))
            {
                features_inner.push(Feature::NoRecursion(NoRecursion));
            }
        }

        let schema_default = self
            .features
            .as_ref()
            .map(|features| features.iter().any(|f| matches!(f, Feature::Default(_))))
            .unwrap_or(false);
        let serde_default = container_rules
            .as_ref()
            .map(|rules| rules.is_default)
            .unwrap_or(false);

        if (schema_default || serde_default) && !flatten {
            let features_inner = field_features.get_or_insert(vec![]);
            if !features_inner
                .iter()
                .any(|f| matches!(f, Feature::Default(_)))
            {
                let field_ident = field
                    .ident
                    .as_ref()
                    .expect("field ident should be exist")
                    .to_owned();
                let default = match container_rules.and_then(|rules| rules.default_path.as_ref()) {
                    Some(path) if !schema_default => {
                        Default(Some(AnyValue::Json(quote! { (#path()).#field_ident })))
                    }
                    _ => Default::new_default_trait(field_ident.into()),
                };
                features_inner.push(Feature::Default(default));
            }
        }

        let rename_field =
            pop_feature!(field_features => Feature::Rename(_)).and_then(|feature| match feature {
                Feature::Rename(rename) => Some(Cow::Owned(rename.into_value())),
                _ => None,
            });

        let schema_deprecated = pop_feature!(field_features => Feature::Deprecated(_)).and_then(
            |feature| match feature {
                Feature::Deprecated(deprecated) => Some(Deprecated::from(deprecated.0)),
                _ => None,
            },
        );
        let deprecated = crate::get_deprecated(&field.attrs).or(schema_deprecated);

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
        let ignore = match pop_feature!(field_features => Feature::Ignore(_)) {
            Some(Feature::Ignore(Ignore(bool_or_exp))) => Some(bool_or_exp),
            _ => None,
        };
        let mut type_tree = override_type_tree.as_ref().unwrap_or(type_tree);
        let is_option = type_tree.is_option();
        while flatten && type_tree.is_option() {
            type_tree = &type_tree.children.as_ref().expect("Option has a child")[0];
        }

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
                    compose_context: self.compose_context.as_ref(),
                };
                if flatten && type_tree.is_map() {
                    Property::FlattenedMap(FlattenedMapSchema::new(cs)?)
                } else {
                    let schema_type = value_type
                        .as_ref()
                        .map(|value_type| &value_type.0)
                        .unwrap_or(&field.ty);
                    Property::Schema(ComponentSchema::for_type(cs, schema_type)?)
                }
            },
            rename_field_value: rename_field,
            required,
            is_option,
            ignore,
        })
    }
}

impl TryToTokens for NamedStructSchema<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let container_rules =
            serde_util::parse_container(self.attributes).map_err(Diagnostic::from)?;
        if container_rules
            .as_ref()
            .is_some_and(|rules| rules.transparent)
        {
            tokens.extend(transparent_schema(
                self.fields,
                self.attributes,
                self.features.clone(),
                self.description.as_ref(),
                &self.struct_name,
                self.compose_context.as_ref(),
            )?);
            return Ok(());
        }
        let schema_default = self
            .features
            .as_ref()
            .map(|features| features.iter().any(|f| matches!(f, Feature::Default(_))))
            .unwrap_or(false);
        let serde_default = container_rules
            .as_ref()
            .map(|rules| rules.is_default)
            .unwrap_or(false);

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

                let field_rule = serde_util::parse_value(&field.attrs).map_err(Diagnostic::from)?;

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

        // Object constraints still apply to the entire instance when this object
        // becomes one member of an allOf for flattened fields.
        let (object_features, schema_features): (Vec<_>, Vec<_>) =
            self.features.iter().flatten().partition(|feature| {
                matches!(
                    feature,
                    Feature::MaxProperties(_) | Feature::MinProperties(_)
                )
            });
        let object_features = object_features
            .into_iter()
            .map(TryToTokens::try_to_token_stream)
            .collect::<DiagResult<TokenStream>>()?;
        let mut object_tokens = quote! { #oapi::oapi::Object::new() #object_features };
        let struct_tag = match container_rules.as_ref().map(|rules| &rules.enum_repr) {
            Some(serde_util::SerdeEnumRepr::InternallyTagged { tag }) => {
                let name = serde_util::parse_value(self.attributes)
                    .map_err(Diagnostic::from)?
                    .and_then(|rules| rules.rename)
                    .unwrap_or_else(|| self.struct_name.trim_start_matches("r#").to_owned());
                object_tokens.extend(quote! {
                    .property(#tag, #oapi::oapi::Object::with_type(#oapi::oapi::schema::BasicType::String).enum_values([#name]))
                    .required(#tag)
                });
                Some((tag, name))
            }
            _ => None,
        };
        for (field, field_rule) in field_values {
            let mut field_name = &*field
                .ident
                .as_ref()
                .expect("field ident should be exists")
                .to_string();

            if let Some(stripped) = field_name.strip_prefix("r#") {
                field_name = stripped;
            }

            let NamedStructFieldOptions {
                property,
                rename_field_value,
                required,
                is_option,
                ignore,
            } = self.field_as_schema_property(field, false, container_rules.as_ref())?;

            // Handle ignore: if ignore is LitBool(true), skip this field entirely
            match &ignore {
                Some(crate::parse_utils::LitBoolOrExprPath::LitBool(lit)) if lit.value() => {
                    continue;
                }
                _ => {}
            }

            let rename_to = field_rule
                .as_ref()
                .and_then(|field_rule| field_rule.rename.as_deref().map(Cow::Borrowed))
                .or(rename_field_value);
            let rename_all = container_rules
                .as_ref()
                .and_then(|container_rule| container_rule.rename_all)
                .or_else(|| {
                    self.rename_all
                        .as_ref()
                        .map(|rename_all| rename_all.to_rename_rule())
                });

            let name = crate::rename::<FieldRename>(field_name, rename_to, rename_all)
                .unwrap_or(Cow::Borrowed(field_name));

            let property = property.try_to_token_stream()?;

            let component_required =
                !is_option && crate::is_required(field_rule.as_ref(), container_rules.as_ref());
            let required = match (required, component_required) {
                (Some(required), _) => required.is_true(),
                (None, component_required) => component_required,
            };

            let mut property_tokens = quote! {
                .property(#name, #property)
            };

            if required {
                property_tokens.extend(quote! {
                    .required(#name)
                })
            }

            match &ignore {
                Some(crate::parse_utils::LitBoolOrExprPath::ExprPath(path)) => {
                    // Wrap in a runtime conditional: only add the property if the function returns
                    // false (i.e., the field is NOT ignored)
                    // We need to switch to a block-based approach for this object
                    object_tokens = quote! {
                        {
                            let __obj = #object_tokens;
                            if !#path() {
                                __obj #property_tokens
                            } else {
                                __obj
                            }
                        }
                    };
                }
                _ => {
                    // LitBool(false) or None: include the field normally
                    object_tokens.extend(property_tokens);
                }
            }
        }

        let flatten_fields: Vec<&Field> = self
            .fields
            .iter()
            .map(|field| {
                let rules = serde_util::parse_value(&field.attrs).map_err(Diagnostic::from)?;
                let skipped = field
                    .attrs
                    .parse_features::<NamedFieldFeatures>()?
                    .into_inner()
                    .is_some_and(|features| features.is_skipped());
                Ok((field, rules, skipped))
            })
            .collect::<DiagResult<Vec<_>>>()?
            .into_iter()
            .filter(|(_, rules, skipped)| {
                !skipped && is_not_skipped(rules.as_ref()) && is_flatten(rules.as_ref())
            })
            .map(|(field, ..)| field)
            .collect();

        let all_of = if !flatten_fields.is_empty() {
            let mut flattened_tokens = quote! { #oapi::oapi::schema::AllOf::new() };
            let mut has_flattened_schema = false;
            let mut flattened_map_field = None;
            let mut flattened_map_schema = None;

            for field in flatten_fields {
                let NamedStructFieldOptions {
                    property,
                    ignore,
                    is_option,
                    ..
                } = self.field_as_schema_property(field, true, container_rules.as_ref())?;

                let guard = match ignore {
                    Some(crate::parse_utils::LitBoolOrExprPath::LitBool(lit)) if lit.value() => {
                        continue;
                    }
                    Some(crate::parse_utils::LitBoolOrExprPath::ExprPath(path)) => Some(path),
                    _ => None,
                };
                match property {
                    Property::Schema(_) | Property::SchemaWith(_) => {
                        let property = property.try_to_token_stream()?;
                        let property = if is_option {
                            quote! { #oapi::oapi::schema::optional_flatten_schema((#property).into(), &__salvo_flatten_base, components) }
                        } else {
                            property
                        };
                        has_flattened_schema = true;
                        flattened_tokens = if let Some(path) = guard {
                            quote! {{ let __all_of = #flattened_tokens;
                                if !#path() { __all_of.item(#property) } else { __all_of }
                            }}
                        } else {
                            quote! { #flattened_tokens.item(#property) }
                        };
                    }
                    Property::FlattenedMap(ref map) => match flattened_map_field {
                        None => {
                            let property = property.try_to_token_stream()?;
                            flattened_map_schema = Some((property, map.key_schema.clone(), guard));
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

            if !has_flattened_schema {
                if let Some((property, key_schema, guard)) = flattened_map_schema {
                    object_tokens = if let Some(path) = guard {
                        quote! {{ let __obj = #object_tokens;
                            if !#path() { #oapi::oapi::schema::with_flattened_map_key(
                                __obj.additional_properties(#property), (#key_schema).into(), components) } else { __obj }
                        }}
                    } else {
                        quote! { #oapi::oapi::schema::with_flattened_map_key(
                        #object_tokens.additional_properties(#property), (#key_schema).into(), components) }
                    };
                }
                tokens.extend(object_tokens);
                false
            } else {
                let schema = quote! {
                    {
                        let __salvo_flatten_base = #object_tokens;
                        (#flattened_tokens).item(__salvo_flatten_base)
                    }
                };
                let schema = if let Some((property, key_schema, guard)) = flattened_map_schema {
                    // additionalProperties only sees this local object's properties.
                    // Evaluate all siblings/references before constraining remaining keys.
                    let catch_all = quote! {
                        .item(#oapi::oapi::Object::new().additional_properties(#property)
                            .property_names(#oapi::oapi::schema::serialized_map_key_schema((#key_schema).into(), components)))
                    };
                    if let Some(path) = guard {
                        quote! {{ let __schema = #schema;
                            if !#path() { __schema #catch_all } else { __schema }
                        }}
                    } else {
                        quote! { #schema #catch_all }
                    }
                } else {
                    schema
                };
                tokens.extend(quote! {
                    #oapi::oapi::schema::flatten_all_of_schema(#schema, components)
                });
                true
            }
        } else {
            tokens.extend(object_tokens);
            false
        };

        if all_of && (schema_default || serde_default) {
            let mut default_value_tokens = TokenStream::new();
            if let Some((tag, name)) = &struct_tag {
                default_value_tokens.extend(quote! {
                    __default.insert(#tag.into(), #oapi::oapi::__private::serde_json::json!(#name));
                });
            }
            let default_constructor = match container_rules
                .as_ref()
                .and_then(|rules| rules.default_path.as_ref())
            {
                Some(path) if !schema_default => quote! { #path() },
                _ => quote! { Self::default() },
            };

            for field in self.fields {
                if field
                    .attrs
                    .parse_features::<NamedFieldFeatures>()?
                    .into_inner()
                    .is_some_and(|features| features.is_skipped())
                {
                    continue;
                }
                let field_rule = serde_util::parse_value(&field.attrs).map_err(Diagnostic::from)?;
                if !is_not_skipped(field_rule.as_ref()) {
                    continue;
                }

                let field_ident = field.ident.as_ref().expect("field ident should be exists");
                let mut field_name = field_ident.to_string();
                if let Some(stripped) = field_name.strip_prefix("r#") {
                    field_name = stripped.to_owned();
                }

                let NamedStructFieldOptions {
                    rename_field_value,
                    ignore,
                    ..
                } = self.field_as_schema_property(
                    field,
                    is_flatten(field_rule.as_ref()),
                    container_rules.as_ref(),
                )?;

                let maybe_guard = match &ignore {
                    Some(crate::parse_utils::LitBoolOrExprPath::LitBool(lit)) if lit.value() => {
                        continue;
                    }
                    Some(crate::parse_utils::LitBoolOrExprPath::ExprPath(path)) => {
                        Some(quote! { if !#path() })
                    }
                    _ => None,
                };

                if is_flatten(field_rule.as_ref()) {
                    let insert_tokens = quote! {
                        if let #oapi::oapi::__private::serde_json::Value::Object(__flattened) =
                            #oapi::oapi::__private::serde_json::to_value((#default_constructor).#field_ident).unwrap()
                        {
                            __default.extend(__flattened);
                        }
                    };
                    if let Some(guard_tokens) = maybe_guard {
                        default_value_tokens.extend(quote! {
                            #guard_tokens {
                                #insert_tokens
                            }
                        });
                    } else {
                        default_value_tokens.extend(insert_tokens);
                    }
                } else {
                    let rename_to = field_rule
                        .as_ref()
                        .and_then(|field_rule| field_rule.rename.as_deref().map(Cow::Borrowed))
                        .or(rename_field_value);
                    let rename_all = container_rules
                        .as_ref()
                        .and_then(|container_rule| container_rule.rename_all)
                        .or_else(|| {
                            self.rename_all
                                .as_ref()
                                .map(|rename_all| rename_all.to_rename_rule())
                        });
                    let name = crate::rename::<FieldRename>(&field_name, rename_to, rename_all)
                        .unwrap_or(Cow::Borrowed(field_name.as_str()))
                        .into_owned();
                    let insert_tokens = quote! {
                        __default.insert(
                            #name.to_owned(),
                            #oapi::oapi::__private::serde_json::to_value((#default_constructor).#field_ident).unwrap(),
                        );
                    };
                    if let Some(guard_tokens) = maybe_guard {
                        default_value_tokens.extend(quote! {
                            #guard_tokens {
                                #insert_tokens
                            }
                        });
                    } else {
                        default_value_tokens.extend(insert_tokens);
                    }
                }
            }

            tokens.extend(quote! {
                .default_value({
                    let mut __default = #oapi::oapi::__private::serde_json::Map::new();
                    #default_value_tokens
                    #oapi::oapi::__private::serde_json::Value::Object(__default)
                })
            });
        }

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

        for feature in schema_features {
            feature.try_to_tokens(tokens)?;
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
    pub(super) compose_context: Option<crate::component::ComposeContext>,
}
impl UnnamedStructSchema<'_> {
    pub(crate) fn pop_skip_bound(&mut self) -> Option<SkipBound> {
        pop_feature_as_inner!(self.features => Feature::SkipBound(_v))
    }
    pub(crate) fn pop_bound(&mut self) -> Option<Bound> {
        pop_feature_as_inner!(self.features => Feature::Bound(_v))
    }
}

// Enumerate distinct serialized subsequences, merging only equivalent item schemas.
// The bound applies to distinct shapes, so homogeneous optional tuples stay linear.
fn conditional_tuple_shapes(
    keys: &[String],
    optional: &[bool],
    span: proc_macro2::Span,
) -> DiagResult<Vec<Vec<usize>>> {
    const MAX_SHAPES: usize = 256;
    let mut shapes = vec![Vec::new()];
    for (index, optional) in optional.iter().enumerate() {
        let mut next = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for shape in shapes {
            let mut included = shape.clone();
            included.push(index);
            for candidate in std::iter::once(included).chain(optional.then_some(shape)) {
                let key = candidate
                    .iter()
                    .map(|index| keys[*index].clone())
                    .collect::<Vec<_>>();
                if seen.insert(key) {
                    next.push(candidate);
                    if next.len() > MAX_SHAPES {
                        return Err(Diagnostic::spanned(span, DiagLevel::Error,
                            "conditional tuple serialization produces more than 256 distinct schema shapes")
                            .help("reduce independently skipped fields, use named fields, or implement ToSchema manually"));
                    }
                }
            }
        }
        shapes = next;
    }
    Ok(shapes)
}

impl TryToTokens for UnnamedStructSchema<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        if serde_util::parse_container(self.attributes)
            .map_err(Diagnostic::from)?
            .is_some_and(|rules| rules.transparent)
        {
            tokens.extend(transparent_schema(
                self.fields,
                self.attributes,
                self.features.clone(),
                self.description.as_ref(),
                &self.struct_name,
                self.compose_context.as_ref(),
            )?);
            return Ok(());
        }
        let oapi = crate::oapi_crate();
        let is_newtype = self.fields.len() == 1;
        let fields = self
            .fields
            .iter()
            .map(|field| {
                let rules = serde_util::parse_value(&field.attrs).map_err(Diagnostic::from)?;
                // Serde's newtype serializer always serializes its single field.
                Ok((is_newtype || is_not_skipped(rules.as_ref())).then_some(field))
            })
            .collect::<DiagResult<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let fields_len = fields.len();
        let Some(first_field) = fields.first() else {
            if is_newtype {
                tokens.extend(
                    quote! { #oapi::oapi::Object::with_type(#oapi::oapi::schema::BasicType::Null) },
                );
            } else {
                tokens.extend(quote! { #oapi::oapi::schema::Array::new().items(#oapi::oapi::schema::ArrayItems::False).min_items(0).max_items(0) });
            }
            return Ok(());
        };
        let first_part = &TypeTree::from_type(&first_field.ty)?;

        let all_fields_are_same = fields_len == 1
            || fields
                .iter()
                .skip(1)
                .map(|field| TypeTree::from_type(&field.ty))
                .collect::<Result<Vec<TypeTree>, Diagnostic>>()?
                .iter()
                .all(|schema_part| first_part == schema_part);

        let deprecated = crate::get_deprecated(self.attributes);
        let optional = fields
            .iter()
            .map(|field| {
                serde_util::parse_value(&field.attrs)
                    .map(|rules| rules.is_some_and(|rules| rules.skip_serializing_if))
                    .map_err(Diagnostic::from)
            })
            .collect::<DiagResult<Vec<_>>>()?;
        if !is_newtype && optional.iter().any(|optional| *optional) {
            let mut features = self.features.clone().unwrap_or_default();
            // These validators describe a scalar/newtype, not a tuple container.
            if let Some(feature) = features.iter().find(|feature| {
                matches!(
                    feature,
                    Feature::MultipleOf(_)
                        | Feature::Maximum(_)
                        | Feature::Minimum(_)
                        | Feature::ExclusiveMaximum(_)
                        | Feature::ExclusiveMinimum(_)
                        | Feature::MaxLength(_)
                        | Feature::MinLength(_)
                        | Feature::Pattern(_)
                        | Feature::MaxItems(_)
                        | Feature::MinItems(_)
                )
            }) {
                return Err(Diagnostic::spanned(
                    self.fields.span(),
                    DiagLevel::Error,
                    format!(
                        "{feature} attribute is not allowed for unnamed structs with multiple fields"
                    ),
                ));
            }
            let value_type = all_fields_are_same
                .then(|| features.pop_value_type_feature())
                .flatten();
            let override_type = value_type
                .as_ref()
                .map(|value| value.as_type_tree())
                .transpose()?;
            let item_format = pop_feature!(features => Feature::Format(_));
            let item_features = features
                .iter()
                .filter(|feature| matches!(feature, Feature::NoRecursion(_)))
                .cloned()
                .chain(item_format)
                .collect::<Vec<_>>();
            let items = fields
                .iter()
                .map(|field| {
                    let type_tree = TypeTree::from_type(&field.ty)?;
                    Ok(ComponentSchema::new(ComponentSchemaProps {
                        type_tree: override_type.as_ref().unwrap_or(&type_tree),
                        features: Some(item_features.clone()),
                        description: None,
                        deprecated: None,
                        object_name: self.struct_name.as_ref(),
                        compose_context: self.compose_context.as_ref(),
                    })?
                    .to_token_stream())
                })
                .collect::<DiagResult<Vec<_>>>()?;
            let keys = items
                .iter()
                .zip(&fields)
                .map(|(item, field)| {
                    // Keep schema/documentation attributes in the equivalence key too,
                    // so future item-level annotations cannot be merged accidentally.
                    let attrs = field
                        .attrs
                        .iter()
                        .filter(|attr| !attr.path().is_ident("serde"));
                    quote! { #item #(#attrs)* }.to_string()
                })
                .collect::<Vec<_>>();
            let shapes = conditional_tuple_shapes(&keys, &optional, self.fields.span())?;
            let variants = shapes.iter().map(|shape| {
                let length = shape.len();
                let prefix = (!shape.is_empty()).then(|| {
                    quote! {
                        .prefix_items([#(__salvo_tuple_items[#shape].clone()),*])
                    }
                });
                quote! {
                    .item(#oapi::oapi::schema::Array::new()
                        .items(#oapi::oapi::schema::ArrayItems::False)
                        #prefix .min_items(#length).max_items(#length))
                }
            });
            let count = items.len();
            let metadata = features
                .iter()
                .map(TryToTokens::try_to_token_stream)
                .collect::<DiagResult<TokenStream>>()?;
            let comments = CommentAttributes::from_attributes(self.attributes);
            let description = self
                .description
                .as_ref()
                .map(ComponentDescription::Description)
                .or(Some(ComponentDescription::CommentAttributes(&comments)));
            let deprecated = deprecated.map(|deprecated| quote! { .deprecated(#deprecated) });
            tokens.extend(quote! {{
                let __salvo_tuple_items: [#oapi::oapi::RefOr<#oapi::oapi::schema::Schema>; #count] =
                    [#(Into::<#oapi::oapi::RefOr<#oapi::oapi::schema::Schema>>::into(#items)),*];
                #oapi::oapi::schema::AnyOf::new() #(#variants)* #metadata #description #deprecated
            }});
            return Ok(());
        }
        if all_fields_are_same {
            let mut unnamed_struct_features = self.features.clone();
            let value_type = unnamed_struct_features
                .as_mut()
                .and_then(|features| features.pop_value_type_feature());
            let override_type_tree = value_type
                .as_ref()
                .map(|value_type| value_type.as_type_tree())
                .transpose()?;

            if is_newtype && let Some(ref mut features) = unnamed_struct_features {
                let inline = parse_schema_features_with(&first_field.attrs, |input| {
                    Ok(parse_features!(input as attributes::Inline))
                })?
                .unwrap_or_default();

                features.extend(inline);

                if pop_feature!(features => Feature::Default(Default(None))).is_some() {
                    let index: syn::Index = 0.into();
                    features.push(Feature::Default(Default::new_default_trait(index.into())));
                }
            }

            // The struct has multiple unnamed fields.
            // Validation related features are only allowed on unnamed structs with a single field
            // (i.e., newtype pattern).
            if !is_newtype
                && let Some(ref mut features) = unnamed_struct_features
                && let Some((name, span)) = features.iter().find_map(|feature| match feature {
                    Feature::MultipleOf(attr) => Some(("multiple_of", attr.span())),
                    Feature::Maximum(attr) => Some(("maximum", attr.span())),
                    Feature::Minimum(attr) => Some(("minimum", attr.span())),
                    Feature::ExclusiveMaximum(attr) => Some(("exclusive_maximum", attr.span())),
                    Feature::ExclusiveMinimum(attr) => Some(("exclusive_minimum", attr.span())),
                    Feature::MaxLength(attr) => Some(("max_length", attr.span())),
                    Feature::MinLength(attr) => Some(("min_length", attr.span())),
                    Feature::Pattern(attr) => Some(("pattern", attr.span())),
                    Feature::MaxItems(attr) => Some(("max_items", attr.span())),
                    Feature::MinItems(attr) => Some(("min_items", attr.span())),
                    _ => None,
                })
            {
                return Err(Diagnostic::spanned(
                    span,
                    DiagLevel::Error,
                    format!(
                        "{name} attribute is not allowed for unnamed structs with multiple fields",
                    ),
                ));
            }

            let comments = CommentAttributes::from_attributes(self.attributes);
            let description = self
                .description
                .as_ref()
                .map(ComponentDescription::Description)
                .or(Some(ComponentDescription::CommentAttributes(&comments)));

            tokens.extend(
                ComponentSchema::for_type(
                    ComponentSchemaProps {
                        type_tree: override_type_tree.as_ref().unwrap_or(first_part),
                        features: unnamed_struct_features,
                        description: description.as_ref(),
                        deprecated: deprecated.as_ref(),
                        object_name: self.struct_name.as_ref(),
                        compose_context: self.compose_context.as_ref(),
                    },
                    value_type
                        .as_ref()
                        .map(|value_type| &value_type.0)
                        .unwrap_or(&first_field.ty),
                )?
                .to_token_stream(),
            );
        } else {
            // JSON Schema 2020-12 describes heterogeneous serde tuples with
            // positional schemas, rather than requiring every item to be an object.
            let items = fields
                .iter()
                .map(|field| {
                    let type_tree = TypeTree::from_type(&field.ty)?;
                    Ok(ComponentSchema::new(ComponentSchemaProps {
                        type_tree: &type_tree,
                        features: None,
                        description: None,
                        deprecated: None,
                        object_name: self.struct_name.as_ref(),
                        compose_context: self.compose_context.as_ref(),
                    })?
                    .to_token_stream())
                })
                .collect::<DiagResult<Vec<_>>>()?;
            tokens.extend(quote! {
                #oapi::oapi::schema::Array::new()
                    .items(#oapi::oapi::schema::ArrayItems::False)
                    .prefix_items([#(Into::<#oapi::oapi::RefOr<#oapi::oapi::schema::Schema>>::into(#items)),*])
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

        if !is_newtype {
            let comments = CommentAttributes::from_attributes(self.attributes);
            let description = self
                .description
                .as_ref()
                .map(ComponentDescription::Description)
                .or(Some(ComponentDescription::CommentAttributes(&comments)));
            if all_fields_are_same {
                tokens.extend(quote! { .to_array() });
            }
            tokens.extend(quote! {
                .max_items(#fields_len)
                .min_items(#fields_len)
                #description
            })
        }
        Ok(())
    }
}

#[cfg(test)]
mod conditional_tuple_tests {
    use proc_macro2::Span;

    use super::conditional_tuple_shapes;

    #[test]
    fn limits_distinct_shapes_without_expanding_homogeneous_combinations() {
        let keys = vec!["same schema".to_owned(); 20];
        let shapes = conditional_tuple_shapes(&keys, &[true; 20], Span::call_site()).unwrap();
        assert_eq!(shapes.len(), 21);
        let keys = (0..9)
            .map(|index| format!("different schema {index}"))
            .collect::<Vec<_>>();
        let error: syn::Error = conditional_tuple_shapes(&keys, &[true; 9], Span::call_site())
            .unwrap_err()
            .into();
        assert!(
            error
                .to_string()
                .contains("more than 256 distinct schema shapes")
        );
        assert!(error.to_string().contains("implement ToSchema manually"));
    }

    #[test]
    fn reports_combinatorial_tuple_expansion_as_a_macro_diagnostic() {
        let input = syn::parse_quote! {
            struct Many(
                #[serde(skip_serializing_if = "never")] [u8; 1],
                #[serde(skip_serializing_if = "never")] [u8; 2],
                #[serde(skip_serializing_if = "never")] [u8; 3],
                #[serde(skip_serializing_if = "never")] [u8; 4],
                #[serde(skip_serializing_if = "never")] [u8; 5],
                #[serde(skip_serializing_if = "never")] [u8; 6],
                #[serde(skip_serializing_if = "never")] [u8; 7],
                #[serde(skip_serializing_if = "never")] [u8; 8],
                #[serde(skip_serializing_if = "never")] [u8; 9],
            );
        };
        let error: syn::Error = crate::schema::to_schema(input).unwrap_err().into();
        assert!(
            error
                .to_string()
                .contains("more than 256 distinct schema shapes")
        );
    }
}
