use std::borrow::Cow;

use proc_macro2::TokenStream;
use proc_macro_error::abort;
use quote::{quote, ToTokens};
use syn::{
    parse::Parse, punctuated::Punctuated, token::Comma, Attribute, Data, Field, Generics, Ident,
};

use crate::{
    component::{
        self,
        features::{
            self, AdditionalProperties, AllowReserved, Example, ExclusiveMaximum, ExclusiveMinimum,
            Explode, Format, Inline, MaxItems, MaxLength, Maximum, MinItems, MinLength, Minimum,
            MultipleOf, Names, Nullable, Pattern, ReadOnly, Rename, RenameAll, SchemaWith, Style,
            WriteOnly, XmlAttr,
        },
        FieldRename,
    },
    doc_comment::CommentAttributes,
    Array, Required, ResultExt,
};

use super::{
    features::{
        impl_into_inner, impl_merge, parse_features, pop_feature, pop_feature_as_inner, Feature,
        FeaturesExt, IntoInner, Merge, ToTokensExt,
    },
    serde::{self, SerdeContainer},
    ComponentSchema, TypeTree,
};

impl_merge!(IntoParametersFeatures, FieldFeatures);

/// Container attribute `#[into_parameters(...)]`.
pub struct IntoParametersFeatures(Vec<Feature>);

impl Parse for IntoParametersFeatures {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self(parse_features!(
            input as Style,
            features::ParameterIn,
            Names,
            RenameAll
        )))
    }
}

impl_into_inner!(IntoParametersFeatures);

#[derive(Debug)]
pub struct IntoParameters {
    /// Attributes tagged on the whole struct or enum.
    pub attrs: Vec<Attribute>,
    /// Generics required to complete the definition.
    pub generics: Generics,
    /// Data within the struct or enum.
    pub data: Data,
    /// Name of the struct or enum.
    pub ident: Ident,
}

impl ToTokens for IntoParameters {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let ident = &self.ident;
        let (impl_generics, ty_generics, where_clause) = self.generics.split_for_impl();

        let mut into_parameters_features = self
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("into_parameters"))
            .map(|attribute| {
                attribute
                    .parse_args::<IntoParametersFeatures>()
                    .unwrap_or_abort()
                    .into_inner()
            })
            .reduce(|acc, item| acc.merge(item));
        let serde_container = serde::parse_container(&self.attrs);

        // #[param] is only supported over fields
        if self.attrs.iter().any(|attr| attr.path().is_ident("param")) {
            abort! {
                ident,
                "found `param` attribute in unsupported context";
                help = "Did you mean `into_parameters`?",
            }
        }

        let names = into_parameters_features.as_mut().and_then(|features| {
            features
                .pop_by(|feature| matches!(feature, Feature::IntoParametersNames(_)))
                .and_then(|feature| match feature {
                    Feature::IntoParametersNames(names) => Some(names.into_values()),
                    _ => None,
                })
        });

        let style = pop_feature!(into_parameters_features => Feature::Style(_));
        let parameter_in = pop_feature!(into_parameters_features => Feature::ParameterIn(_));
        let rename_all = pop_feature!(into_parameters_features => Feature::RenameAll(_));

        let params = self
            .get_struct_fields(&names.as_ref())
            .enumerate()
            .map(|(index, field)| {
                Param {
                    field,
                    container_attributes: FieldParamContainerAttributes {
                        rename_all: rename_all.as_ref().and_then(|feature| {
                            match feature {
                                Feature::RenameAll(rename_all) => Some(rename_all),
                                _ => None
                            }
                        }),
                        style: &style,
                        parameter_in: &parameter_in,
                        name: names.as_ref()
                            .map(|names| names.get(index).unwrap_or_else(|| abort!(
                                ident,
                                "There is no name specified in the names(...) container attribute for tuple struct field {}",
                                index
                            ))),
                    },
                    serde_container: serde_container.as_ref(),
                }
            })
            .collect::<Array<Param>>();

        let oapi = crate::oapi_crate();
        tokens.extend(quote! {
            impl #impl_generics #oapi::oapi::IntoParameters for #ident #ty_generics #where_clause {
                fn into_parameters(parameter_in_provider: impl Fn() -> Option<#oapi::oapi::parameter::ParameterIn>) -> Vec<#oapi::oapi::parameter::Parameter> {
                    #params.to_vec()
                }
            }
        });
    }
}

impl IntoParameters {
    fn get_struct_fields(
        &self,
        field_names: &Option<&Vec<String>>,
    ) -> impl Iterator<Item = &Field> {
        let ident = &self.ident;
        let abort = |note: &str| {
            abort! {
                ident,
                "unsupported data type, expected struct with named fields `struct {} {{...}}` or unnamed fields `struct {}(...)`",
                ident.to_string(),
                ident.to_string();
                note = note
            }
        };

        match &self.data {
            Data::Struct(data_struct) => match &data_struct.fields {
                syn::Fields::Named(named_fields) => {
                    if field_names.is_some() {
                        abort! {ident, "`#[into_parameters(names(...))]` is not supported attribute on a struct with named fields"}
                    }
                    named_fields.named.iter()
                }
                syn::Fields::Unnamed(unnamed_fields) => {
                    self.validate_unnamed_field_names(&unnamed_fields.unnamed, field_names);
                    unnamed_fields.unnamed.iter()
                }
                _ => abort("Unit type struct is not supported"),
            },
            _ => abort("Only struct type is supported"),
        }
    }

    fn validate_unnamed_field_names(
        &self,
        unnamed_fields: &Punctuated<Field, Comma>,
        field_names: &Option<&Vec<String>>,
    ) {
        let ident = &self.ident;
        match field_names {
            Some(names) => {
                if names.len() != unnamed_fields.len() {
                    abort! {
                        ident,
                        "declared names amount '{}' does not match to the unnamed fields amount '{}' in type: {}",
                            names.len(), unnamed_fields.len(), ident;
                        help = r#"Did you forget to add a field name to `#[into_parameters(names(... , "field_name"))]`"#;
                        help = "Or have you added extra name but haven't defined a type?"
                    }
                }
            }
            None => {
                abort! {
                    ident,
                    "struct with unnamed fields must have explicit name declarations.";
                    help = "Try defining `#[into_parameters(names(...))]` over your type: {}", ident,
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct FieldParamContainerAttributes<'a> {
    /// See [`IntoParametersAttr::style`].
    style: &'a Option<Feature>,
    /// See [`IntoParametersAttr::names`]. The name that applies to this field.
    name: Option<&'a String>,
    /// See [`IntoParametersAttr::parameter_in`].
    parameter_in: &'a Option<Feature>,
    /// Custom rename all if serde attribute is not present.
    rename_all: Option<&'a RenameAll>,
}

struct FieldFeatures(Vec<Feature>);

impl_into_inner!(FieldFeatures);

impl Parse for FieldFeatures {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self(parse_features!(
            // param features
            input as component::features::ValueType,
            Rename,
            Style,
            AllowReserved,
            Example,
            Explode,
            SchemaWith,
            component::features::Required,
            // param schema features
            Inline,
            Format,
            component::features::Default,
            WriteOnly,
            ReadOnly,
            Nullable,
            XmlAttr,
            MultipleOf,
            Maximum,
            Minimum,
            ExclusiveMaximum,
            ExclusiveMinimum,
            MaxLength,
            MinLength,
            Pattern,
            MaxItems,
            MinItems,
            AdditionalProperties
        )))
    }
}

#[derive(Debug)]
struct Param<'a> {
    /// Field in the container used to create a single parameter.
    field: &'a Field,
    /// Attributes on the container which are relevant for this macro.
    container_attributes: FieldParamContainerAttributes<'a>,
    /// Either serde rename all rule or into_parameters rename all rule if provided.
    serde_container: Option<&'a SerdeContainer>,
}

impl Param<'_> {
    /// Resolve [`Param`] features and split features into two [`Vec`]s. Features are split by
    /// whether they should be rendered in [`Param`] itself or in [`Param`]s schema.
    ///
    /// Method returns a tuple containing two [`Vec`]s of [`Feature`].
    fn resolve_field_features(&self) -> (Vec<Feature>, Vec<Feature>) {
        let mut field_features = self
            .field
            .attrs
            .iter()
            .filter(|attribute| attribute.path().is_ident("param"))
            .map(|attribute| {
                attribute
                    .parse_args::<FieldFeatures>()
                    .unwrap_or_abort()
                    .into_inner()
            })
            .reduce(|acc, item| acc.merge(item))
            .unwrap_or_default();

        if let Some(ref style) = self.container_attributes.style {
            if !field_features
                .iter()
                .any(|feature| matches!(&feature, Feature::Style(_)))
            {
                field_features.push(style.clone()); // could try to use cow to avoid cloning
            };
        }

        field_features.into_iter().fold(
            (Vec::<Feature>::new(), Vec::<Feature>::new()),
            |(mut schema_features, mut param_features), feature| {
                match feature {
                    Feature::Inline(_)
                    | Feature::Format(_)
                    | Feature::Default(_)
                    | Feature::WriteOnly(_)
                    | Feature::ReadOnly(_)
                    | Feature::Nullable(_)
                    | Feature::XmlAttr(_)
                    | Feature::MultipleOf(_)
                    | Feature::Maximum(_)
                    | Feature::Minimum(_)
                    | Feature::ExclusiveMaximum(_)
                    | Feature::ExclusiveMinimum(_)
                    | Feature::MaxLength(_)
                    | Feature::MinLength(_)
                    | Feature::Pattern(_)
                    | Feature::MaxItems(_)
                    | Feature::MinItems(_)
                    | Feature::AdditionalProperties(_) => {
                        schema_features.push(feature);
                    }
                    _ => {
                        param_features.push(feature);
                    }
                };

                (schema_features, param_features)
            },
        )
    }
}

impl ToTokens for Param<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        let field = self.field;
        let ident = &field.ident;
        let mut name = &*ident
            .as_ref()
            .map(|ident| ident.to_string())
            .or_else(|| self.container_attributes.name.cloned())
            .unwrap_or_else(|| abort!(
                field, "No name specified for unnamed field.";
                help = "Try adding #[into_parameters(names(...))] container attribute to specify the name for this field"
            ));

        if name.starts_with("r#") {
            name = &name[2..];
        }

        let field_param_serde = serde::parse_value(&field.attrs);

        let (schema_features, mut param_features) = self.resolve_field_features();

        let rename = param_features
            .pop_rename_feature()
            .map(|rename| rename.into_value());
        let rename_to = field_param_serde
            .as_ref()
            .and_then(|field_param_serde| field_param_serde.rename.as_deref().map(Cow::Borrowed))
            .or_else(|| rename.map(Cow::Owned));
        let rename_all = self
            .serde_container
            .as_ref()
            .and_then(|serde_container| serde_container.rename_all.as_ref())
            .or_else(|| {
                self.container_attributes
                    .rename_all
                    .map(|rename_all| rename_all.as_rename_rule())
            });
        let name = super::rename::<FieldRename>(name, rename_to, rename_all)
            .unwrap_or(Cow::Borrowed(name));
        let type_tree = TypeTree::from_type(&field.ty);

        tokens.extend(quote! { #oapi::oapi::parameter::Parameter::new()
            .name(#name)
        });
        tokens.extend(
            if let Some(ref parameter_in) = self.container_attributes.parameter_in {
                parameter_in.into_token_stream()
            } else {
                quote! {
                    .parameter_in(parameter_in_provider().unwrap_or_default())
                }
            },
        );

        if let Some(deprecated) = super::get_deprecated(&field.attrs) {
            tokens.extend(quote! { .deprecated(Some(#deprecated)) });
        }

        let schema_with = pop_feature!(param_features => Feature::SchemaWith(_));
        if let Some(schema_with) = schema_with {
            tokens.extend(quote! { .schema(Some(#schema_with)) });
        } else {
            let description =
                CommentAttributes::from_attributes(&field.attrs).as_formatted_string();
            if !description.is_empty() {
                tokens.extend(quote! { .description(#description)})
            }

            let value_type = param_features.pop_value_type_feature();
            let component = value_type
                .as_ref()
                .map(|value_type| value_type.as_type_tree())
                .unwrap_or(type_tree);

            let required = pop_feature_as_inner!(param_features => Feature::Required(_v))
                .as_ref()
                .map(super::features::Required::is_true)
                .unwrap_or(false);

            let non_required = (component.is_option() && !required)
                || !component::is_required(field_param_serde.as_ref(), self.serde_container);
            let required: Required = (!non_required).into();

            tokens.extend(quote! {
                .required(#required)
            });
            tokens.extend(param_features.to_token_stream());

            let schema = ComponentSchema::new(component::ComponentSchemaProps {
                type_tree: &component,
                features: Some(schema_features),
                description: None,
                deprecated: None,
                object_name: "",
            });

            tokens.extend(quote! { .schema(Some(#schema)) });
        }
    }
}
