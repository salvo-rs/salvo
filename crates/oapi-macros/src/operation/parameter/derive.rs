use std::borrow::Cow;

use proc_macro2::{Span, TokenStream};
use proc_macro_error::abort;
use quote::{format_ident, quote, ToTokens};
use syn::{
    parse::Parse, punctuated::Punctuated, token::Comma, Attribute, Data, Field, GenericParam, Generics, Ident,
    Lifetime, LifetimeParam,
};

use crate::{
    component::{
        self,
        features::{
            self, impl_into_inner, impl_merge, parse_features, pop_feature, pop_feature_as_inner, AdditionalProperties,
            AllowReserved, Example, ExclusiveMaximum, ExclusiveMinimum, Explode, Feature, FeaturesExt, Format, Inline,
            IntoInner, MaxItems, MaxLength, Maximum, Merge, MinItems, MinLength, Minimum, MultipleOf, Names, Nullable,
            Pattern, ReadOnly, Rename, RenameAll, SchemaWith, Style, ToTokensExt, WriteOnly, XmlAttr,
        },
        serde::{self, RenameRule, SerdeContainer},
        ComponentSchema, FieldRename, TypeTree,
    },
    doc_comment::CommentAttributes,
    operation::ParameterIn,
    Array, Required, ResultExt,
};

impl_merge!(AsParametersFeatures, FieldFeatures);

/// Container attribute `#[as_parameters(...)]`.
pub struct AsParametersFeatures(Vec<Feature>);

impl Parse for AsParametersFeatures {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self(parse_features!(
            input as Style,
            features::ParameterIn,
            Names,
            RenameAll
        )))
    }
}

impl_into_inner!(AsParametersFeatures);

#[derive(Debug)]
pub struct AsParameters {
    /// Attributes tagged on the whole struct or enum.
    pub attrs: Vec<Attribute>,
    /// Generics required to complete the definition.
    pub generics: Generics,
    /// Data within the struct or enum.
    pub data: Data,
    /// Name of the struct or enum.
    pub ident: Ident,
}

impl ToTokens for AsParameters {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let ident = &self.ident;
        let salvo = crate::salvo_crate();
        let oapi = crate::oapi_crate();
        let (impl_generics, ty_generics, where_clause) = self.generics.split_for_impl();

        let de_life = &Lifetime::new("'__de", Span::call_site());
        let de_lifetime: GenericParam = LifetimeParam::new(de_life.clone()).into();
        let mut de_generics = self.generics.clone();
        de_generics.params.insert(0, de_lifetime);
        let de_impl_generics = de_generics.split_for_impl().0;

        let mut as_parameters_features = self
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("as_parameters"))
            .map(|attribute| {
                attribute
                    .parse_args::<AsParametersFeatures>()
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
                help = "Did you mean `as_parameters`?",
            }
        }

        let names = as_parameters_features.as_mut().and_then(|features| {
            features
                .pop_by(|feature| matches!(feature, Feature::AsParametersNames(_)))
                .and_then(|feature| match feature {
                    Feature::AsParametersNames(names) => Some(names.into_values()),
                    _ => None,
                })
        });

        let style = pop_feature!(as_parameters_features => Feature::Style(_));
        let parameter_in = pop_feature!(as_parameters_features => Feature::ParameterIn(_));
        let rename_all = pop_feature!(as_parameters_features => Feature::RenameAll(_));
        let source_from = if let Some(Feature::ParameterIn(features::ParameterIn(parameter_in))) = parameter_in {
            match parameter_in {
                ParameterIn::Query => quote! {  #salvo::extract::metadata::SourceFrom::Query },
                ParameterIn::Header => quote! {  #salvo::extract::metadata::SourceFrom::Header },
                ParameterIn::Path => quote! { #salvo::extract::metadata::SourceFrom::Param },
                ParameterIn::Cookie => quote! {  #salvo::extract::metadata::SourceFrom::Cookie },
            }
        } else {
            quote! { #salvo::extract::metadata::SourceFrom::Query }
        };
        let default_source = quote! { #salvo::extract::metadata::Source::new(#source_from, #salvo::extract::metadata::SourceFormat::MultiMap) };
        let fields = self
        .get_struct_fields(&names.as_ref())
        .enumerate()
        .map(|(index, field)| {
            let name = if let Some(ident) = field.ident.as_ref() {
                ident.to_string()
            } else if let Some(name) = names.as_ref().and_then(|names|names.get(index)) {
                name.to_string()
            } else {
                abort! {
                    field,
                    "tuple structs are not supported";
                    help = "consider using a struct with named fields instead, or use `#[as_parameters(names(\"...\"))]` to specify a name for each field",
                }
            };
            quote!{ #salvo::extract::metadata::Field{
                name: #name,
                sources: vec![],
                aliases: vec![],
                metadata: None,
                rename: None,
            }}
        })
        .collect::<Vec<_>>();
        let params = self
            .get_struct_fields(&names.as_ref())
            .enumerate()
            .map(|(index, field)| {
                Parameter {
                    field,
                    container_attributes: FieldParameterContainerAttributes {
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
            .collect::<Array<Parameter>>();

        let rename_all = rename_all
            .as_ref()
            .map(|feature| match feature {
                Feature::RenameAll(RenameAll(rename_rule)) => match rename_rule {
                    RenameRule::Lower => quote! { Some(#salvo::extract::metadata::RenameRule::LowerCase) },
                    RenameRule::Upper => quote! { Some(#salvo::extract::metadata::RenameRule::UpperCase) },
                    RenameRule::Camel => quote! { Some(#salvo::extract::metadata::RenameRule::CamelCase) },
                    RenameRule::Snake => quote! { Some(#salvo::extract::metadata::RenameRule::SnakeCase) },
                    RenameRule::ScreamingSnake => {
                        quote! { Some(#salvo::extract::metadata::RenameRule::ScreamingSnakeCase) }
                    }
                    RenameRule::Pascal => quote! { Some(#salvo::extract::metadata::RenameRule::LowerCase) },
                    RenameRule::Kebab => quote! { Some(#salvo::extract::metadata::RenameRule::KebabCase) },
                    RenameRule::ScreamingKebab => {
                        quote! { Some(#salvo::extract::metadata::RenameRule::ScreamingKebabCase) }
                    }
                },
                _ => quote! {None},
            })
            .unwrap_or_else(|| quote! {None});
        let name = ident.to_string();
        let metadata: Ident = format_ident!("__salvo_extract_{}", name);
        tokens.extend(quote! {
            impl #de_impl_generics #oapi::oapi::AsParameters<'__de> for #ident #ty_generics #where_clause {
                fn parameters() -> #oapi::oapi::Parameters {
                    #oapi::oapi::Parameters(#params.to_vec())
                }
            }

            #[#salvo::async_trait]
            impl #impl_generics #oapi::oapi::EndpointModifier for #ident #ty_generics #where_clause {
                fn modify(_components: &mut #oapi::oapi::Components, operation: &mut #oapi::oapi::Operation) {
                    for parameter in <Self as #oapi::oapi::AsParameters>::parameters() {
                        operation.parameters.insert(parameter);
                    }
                }
            }
            #[allow(non_upper_case_globals)]
            static #metadata: #salvo::__private::once_cell::sync::Lazy<#salvo::extract::Metadata> = #salvo::__private::once_cell::sync::Lazy::new(||
                #salvo::extract::Metadata {
                    name: #name,
                    default_sources: vec![#default_source],
                    fields: vec![#(#fields),*],
                    rename_all: #rename_all,
                });
            #[#salvo::async_trait]
            impl #de_impl_generics #salvo::Extractible<'__de> for #ident #ty_generics #where_clause {
                fn metadata() -> &'__de #salvo::extract::Metadata {
                    &*#metadata
                }
                async fn extract(req: &'__de mut #salvo::Request) -> Result<Self, #salvo::http::ParseError> {
                    #salvo::serde::from_request(req, Self::metadata()).await
                }
                async fn extract_with_arg(req: &'__de mut #salvo::Request, _arg: &str) -> Result<Self, #salvo::http::ParseError> {
                    Self::extract(req).await
                }
            }
        });
    }
}

impl AsParameters {
    fn get_struct_fields(&self, field_names: &Option<&Vec<String>>) -> impl Iterator<Item = &Field> {
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
                        abort! {ident, "`#[as_parameters(names(...))]` is not supported attribute on a struct with named fields"}
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
                        help = r#"Did you forget to add a field name to `#[as_parameters(names(... , "field_name"))]`"#;
                        help = "Or have you added extra name but haven't defined a type?"
                    }
                }
            }
            None => {
                abort! {
                    ident,
                    "struct with unnamed fields must have explicit name declarations.";
                    help = "Try defining `#[as_parameters(names(...))]` over your type: {}", ident,
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct FieldParameterContainerAttributes<'a> {
    /// See [`AsParametersAttr::style`].
    style: &'a Option<Feature>,
    /// See [`AsParametersAttr::names`]. The name that applies to this field.
    name: Option<&'a String>,
    /// See [`AsParametersAttr::parameter_in`].
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
struct Parameter<'a> {
    /// Field in the container used to create a single parameter.
    field: &'a Field,
    /// Attributes on the container which are relevant for this macro.
    container_attributes: FieldParameterContainerAttributes<'a>,
    /// Either serde rename all rule or as_parameters rename all rule if provided.
    serde_container: Option<&'a SerdeContainer>,
}

impl Parameter<'_> {
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
            .map(|attribute| attribute.parse_args::<FieldFeatures>().unwrap_or_abort().into_inner())
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

impl ToTokens for Parameter<'_> {
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
                help = "Try adding #[as_parameters(names(...))] container attribute to specify the name for this field"
            ));

        if name.starts_with("r#") {
            name = &name[2..];
        }

        let field_param_serde = serde::parse_value(&field.attrs);

        let (schema_features, mut param_features) = self.resolve_field_features();

        let rename = param_features.pop_rename_feature().map(|rename| rename.into_value());
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
        let name = crate::component::rename::<FieldRename>(name, rename_to, rename_all).unwrap_or(Cow::Borrowed(name));
        let type_tree = TypeTree::from_type(&field.ty);

        tokens.extend(quote! { #oapi::oapi::parameter::Parameter::new(#name)});
        if let Some(ref parameter_in) = self.container_attributes.parameter_in {
            tokens.extend(parameter_in.into_token_stream());
        }

        if let Some(deprecated) = crate::component::get_deprecated(&field.attrs) {
            tokens.extend(quote! { .deprecated(#deprecated) });
        }

        let schema_with = pop_feature!(param_features => Feature::SchemaWith(_));
        if let Some(schema_with) = schema_with {
            tokens.extend(quote! { .schema(#schema_with) });
        } else {
            let description = CommentAttributes::from_attributes(&field.attrs).as_formatted_string();
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
                .map(crate::component::features::Required::is_true)
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

            tokens.extend(quote! { .schema(#schema) });
        }
    }
}
