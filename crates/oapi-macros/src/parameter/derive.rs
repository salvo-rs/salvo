use std::borrow::Cow;

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::spanned::Spanned;
use syn::{
    Attribute, Data, Field, GenericParam, Generics, Ident, Lifetime, LifetimeParam, Token,
    parse::Parse, punctuated::Punctuated,
};

use crate::component::{self, ComponentSchema};
use crate::doc_comment::CommentAttributes;
use crate::feature::attributes::{
    self, AdditionalProperties, AllowReserved, DefaultParameterIn, DefaultStyle, Example, Explode,
    Format, Inline, Nullable, ReadOnly, Rename, RenameAll, SchemaWith, Style, ToParametersNames,
    ValueType, WriteOnly, XmlAttr,
};
use crate::feature::validation::{
    ExclusiveMaximum, ExclusiveMinimum, MaxItems, MaxLength, Maximum, MinItems, MinLength, Minimum,
    MultipleOf, Pattern,
};
use crate::feature::{
    Feature, FeaturesExt, Merge, TryToTokensExt, impl_into_inner, impl_merge, parse_features,
    pop_feature,
};
use crate::parameter::ParameterIn;
use crate::serde_util::{self, RenameRule, SerdeContainer, SerdeValue};
use crate::type_tree::TypeTree;
use crate::{
    Array, DiagLevel, DiagResult, Diagnostic, FieldRename, IntoInner, Required, TryToTokens,
    attribute,
};

impl_merge!(ToParametersFeatures, FieldFeatures);

/// Container attribute `#[salvo(parameters(...))]`.
pub(crate) struct ToParametersFeatures(Vec<Feature>);

impl Parse for ToParametersFeatures {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self(parse_features!(
            input as DefaultStyle,
            DefaultParameterIn,
            ToParametersNames,
            RenameAll
        )))
    }
}

impl_into_inner!(ToParametersFeatures);

#[derive(Debug)]
pub(crate) struct ToParameters {
    /// Attributes tagged on the whole struct or enum.
    pub(crate) attrs: Vec<Attribute>,
    /// Generics required to complete the definition.
    pub(crate) generics: Generics,
    /// Data within the struct or enum.
    pub(crate) data: Data,
    /// Name of the struct or enum.
    pub(crate) ident: Ident,
}

impl TryToTokens for ToParameters {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let ident = &self.ident;
        let salvo = crate::salvo_crate();
        let oapi = crate::oapi_crate();
        let (impl_generics, ty_generics, where_clause) = self.generics.split_for_impl();

        let ex_life = &Lifetime::new("'__macro_gen_ex", Span::call_site());
        let ex_lifetime: GenericParam = LifetimeParam::new(ex_life.clone()).into();
        let mut ex_generics = self.generics.clone();
        ex_generics.params.insert(0, ex_lifetime);
        let ex_impl_generics = ex_generics.split_for_impl().0;

        let mut parameters_features = self
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("salvo"))
            .filter_map(|attr| {
                attribute::find_nested_list(attr, "parameters")
                    .ok()
                    .flatten()
            })
            .map(|meta| {
                meta.parse_args::<ToParametersFeatures>()
                    .map_err(Diagnostic::from)
                    .map(ToParametersFeatures::into_inner)
            })
            .collect::<Result<Vec<_>, Diagnostic>>()?
            .into_iter()
            .reduce(|acc, item| acc.merge(item));
        let serde_container = serde_util::parse_container(&self.attrs);

        // #[param] is only supported over fields
        if self.attrs.iter().any(|attr| {
            attr.path().is_ident("salvo")
                && attribute::find_nested_list(attr, "parameter")
                    .ok()
                    .flatten()
                    .is_some()
        }) {
            return Err(Diagnostic::spanned(
                ident.span(),
                DiagLevel::Error,
                "found `parameter` attribute in unsupported context",
            )
            .help("Did you mean `parameters`?"));
        }

        let names = parameters_features.as_mut().and_then(|features| {
            let to_parameters_names = pop_feature!(features => Feature::ToParametersNames(_));
            IntoInner::<Option<ToParametersNames>>::into_inner(to_parameters_names)
                .map(|names| names.into_values())
        });

        let default_style = pop_feature!(parameters_features => Feature::DefaultStyle(_));
        let default_parameter_in =
            pop_feature!(parameters_features => Feature::DefaultParameterIn(_));
        let rename_all = pop_feature!(parameters_features => Feature::RenameAll(_));
        let default_source_from =
            if let Some(Feature::DefaultParameterIn(DefaultParameterIn(default_parameter_in))) =
                default_parameter_in
            {
                match default_parameter_in {
                    ParameterIn::Query => quote! { #salvo::extract::metadata::SourceFrom::Query },
                    ParameterIn::Header => quote! { #salvo::extract::metadata::SourceFrom::Header },
                    ParameterIn::Path => quote! { #salvo::extract::metadata::SourceFrom::Param },
                    ParameterIn::Cookie => quote! { #salvo::extract::metadata::SourceFrom::Cookie },
                }
            } else {
                quote! { #salvo::extract::metadata::SourceFrom::Query }
            };
        let default_source = quote! { #salvo::extract::metadata::Source::new(#default_source_from, #salvo::extract::metadata::SourceParser::MultiMap) };
        let params = self
            .get_struct_fields(&names.as_ref())?
            .enumerate()
            .filter_map(|(index, field)| {
                let field_serde_params = serde_util::parse_value(&field.attrs);
                if matches!(&field_serde_params, Some(params) if !params.skip) {
                    Some((index, field, field_serde_params))
                } else {
                    None
                }
            })
            .map(|(index, field, field_serde_params)|{
                Ok(Parameter {
                    field,
                    field_serde_params,
                    container_attributes: FieldParameterContainerAttributes {
                        rename_all: rename_all.as_ref().and_then(|feature| {
                            match feature {
                                Feature::RenameAll(rename_all) => Some(rename_all),
                                _ => None
                            }
                        }),
                        default_style: &default_style,
                        default_parameter_in: &default_parameter_in,
                        name: names.as_ref().map(|names| names.get(index).ok_or_else(||  Diagnostic::spanned(
                            ident.span(),
                            DiagLevel::Error,
                            format!("There is no name specified in the names(...) container attribute for tuple struct field {}", index)
                        ))).transpose()?,
                    },
                    serde_container: serde_container.as_ref(),
                })
            })
            .collect::<DiagResult<Vec<Parameter>>>()?;

        let extract_fields = if self.is_named_struct() {
            params
                .iter()
                .map(|param| param.to_extract_field_token_stream(&salvo))
                .collect::<DiagResult<Vec<TokenStream>>>()?
        } else if let Some(names) = &names {
            names
                .iter()
                .map(|name| quote! { #salvo::extract::metadata::Field::new(#name)})
                .collect::<Vec<_>>()
        } else {
            vec![]
        };

        fn quote_rename_rule(salvo: &Ident, rename_all: &RenameRule) -> TokenStream {
            let rename_all = match rename_all {
                RenameRule::LowerCase => "LowerCase",
                RenameRule::UpperCase => "UpperCase",
                RenameRule::PascalCase => "PascalCase",
                RenameRule::CamelCase => "CamelCase",
                RenameRule::SnakeCase => "SnakeCase",
                RenameRule::ScreamingSnakeCase => "ScreamingSnakeCase",
                RenameRule::KebabCase => "KebabCase",
                RenameRule::ScreamingKebabCase => "ScreamingKebabCase",
            };
            let rule = Ident::new(rename_all, Span::call_site());
            quote! {
                #salvo::extract::RenameRule::#rule
            }
        }
        let rename_all = rename_all
            .as_ref()
            .map(|feature| match feature {
                Feature::RenameAll(RenameAll(rename_rule)) => {
                    let rule = quote_rename_rule(&salvo, rename_rule);
                    Some(quote! {
                        .rename_all(#rule)
                    })
                }
                _ => None,
            })
            .unwrap_or_else(|| None);
        let serde_rename_all = if let Some(serde_rename_all) = serde_container
            .as_ref()
            .and_then(|container| container.rename_all)
        {
            let rule = quote_rename_rule(&salvo, &serde_rename_all);
            Some(quote! {
                .serde_rename_all(#rule)
            })
        } else {
            None
        };

        let name = ident.to_string();
        let params = params
            .iter()
            .map(TryToTokens::try_to_token_stream)
            .collect::<DiagResult<Array<TokenStream>>>()?;
        tokens.extend(quote!{
            impl #ex_impl_generics #oapi::oapi::ToParameters<'__macro_gen_ex> for #ident #ty_generics #where_clause {
                fn to_parameters(components: &mut #oapi::oapi::Components) -> #oapi::oapi::Parameters {
                    #oapi::oapi::Parameters(#params.to_vec())
                }
            }
            impl #impl_generics #oapi::oapi::EndpointArgRegister for #ident #ty_generics #where_clause {
                fn register(components: &mut #oapi::oapi::Components, operation: &mut #oapi::oapi::Operation, _arg: &str) {
                    for parameter in <Self as #oapi::oapi::ToParameters>::to_parameters(components) {
                        operation.parameters.insert(parameter);
                    }
                }
            }
            impl #ex_impl_generics #salvo::Extractible<'__macro_gen_ex> for #ident #ty_generics #where_clause {
                fn metadata() -> &'__macro_gen_ex #salvo::extract::Metadata {
                    static METADATA: ::std::sync::OnceLock<#salvo::extract::Metadata> = ::std::sync::OnceLock::new();
                    METADATA.get_or_init(||
                        #salvo::extract::Metadata::new(#name)
                            .default_sources(vec![#default_source])
                            .fields(vec![#(#extract_fields),*])
                            #rename_all
                            #serde_rename_all
                    )
                }
                async fn extract(req: &'__macro_gen_ex mut #salvo::Request) -> Result<Self, impl #salvo::Writer + Send + std::fmt::Debug + 'static> {
                    #salvo::serde::from_request(req, Self::metadata()).await
                }
                async fn extract_with_arg(req: &'__macro_gen_ex mut #salvo::Request, _arg: &str) -> Result<Self, impl #salvo::Writer + Send + std::fmt::Debug + 'static> {
                    Self::extract(req).await
                }
            }
        });
        Ok(())
    }
}

impl ToParameters {
    fn is_named_struct(&self) -> bool {
        matches!(&self.data, Data::Struct(data_struct) if matches!(&data_struct.fields, syn::Fields::Named(_)))
    }
    fn get_struct_fields(
        &self,
        field_names: &Option<&Vec<String>>,
    ) -> DiagResult<impl Iterator<Item = &Field>> {
        let ident = &self.ident;
        let abort = |note: &str| {
            let msg = format!(
                "unsupported data type, expected struct with named fields `struct {} {{...}}` or unnamed fields `struct {}(...)`",
                ident, ident
            );
            Err(Diagnostic::spanned(ident.span(), DiagLevel::Error, msg).note(note))
        };

        match &self.data {
            Data::Struct(data_struct) => match &data_struct.fields {
                syn::Fields::Named(named_fields) => {
                    if field_names.is_some() {
                        return Err(Diagnostic::spanned(
                            ident.span(),
                            DiagLevel::Error,
                            "`#[salvo(parameters(names(...)))]` is not supported attribute on a struct with named fields",
                        ));
                    }
                    Ok(named_fields.named.iter())
                }
                syn::Fields::Unnamed(unnamed_fields) => {
                    self.validate_unnamed_field_names(&unnamed_fields.unnamed, field_names)?;
                    Ok(unnamed_fields.unnamed.iter())
                }
                _ => abort("Unit type struct is not supported"),
            },
            _ => abort("Only struct type is supported"),
        }
    }

    fn validate_unnamed_field_names(
        &self,
        unnamed_fields: &Punctuated<Field, Token![,]>,
        field_names: &Option<&Vec<String>>,
    ) -> DiagResult<()> {
        let ident = &self.ident;
        match field_names {
            Some(names) => {
                if names.len() != unnamed_fields.len() {
                    Err(Diagnostic::spanned(
                        ident.span(),
                        DiagLevel::Error,
                        format!(
                            "declared names amount '{}' does not match to the unnamed fields amount '{}' in type: {}",
                            names.len(),
                            unnamed_fields.len(),
                            ident
                        ),
                    )
                    .help(
                        r#"Did you forget to add a field name to `#[salvo(parameters(names(... , "field_name")))]`
                    Or have you added extra name but haven't defined a type?"#,
                    ))
                } else {
                    Ok(())
                }
            }
            None => Err(Diagnostic::spanned(
                ident.span(),
                DiagLevel::Error,
                "struct with unnamed fields must have explicit name declarations.",
            )
            .help(format!(
                "Try defining `#[salvo(parameters(names(...)))]` over your type: {}",
                ident
            ))),
        }
    }
}

#[derive(Debug)]
pub(crate) struct FieldParameterContainerAttributes<'a> {
    /// See [`ToParameterAttr::style`].
    default_style: &'a Option<Feature>,
    /// See [`ToParametersAttr::names`]. The name that applies to this field.
    name: Option<&'a String>,
    /// See [`ToParametersAttr::parameter_in`].
    default_parameter_in: &'a Option<Feature>,
    /// Custom rename all if serde attribute is not present.
    rename_all: Option<&'a RenameAll>,
}

struct FieldFeatures(Vec<Feature>);

impl_into_inner!(FieldFeatures);

impl Parse for FieldFeatures {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self(parse_features!(
            // param features
            input as ValueType,
            Rename,
            Style,
            attributes::ParameterIn,
            AllowReserved,
            Example,
            Explode,
            SchemaWith,
            attributes::Required,
            // param schema features
            Inline,
            Format,
            attributes::Default,
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
    //// Field serde params parsed from field attributes.
    field_serde_params: Option<SerdeValue>,
    /// Attributes on the container which are relevant for this macro.
    container_attributes: FieldParameterContainerAttributes<'a>,
    /// Either serde rename all rule or to_parameters rename all rule if provided.
    serde_container: Option<&'a SerdeContainer>,
}

impl Parameter<'_> {
    /// Resolve [`Parameter`] features and split features into two [`Vec`]s. Features are split by
    /// whether they should be rendered in [`Parameter`] itself or in [`Parameter`]s schema.
    ///
    /// Method returns a tuple containing two [`Vec`]s of [`Feature`].
    fn resolve_field_features(&self) -> DiagResult<(Vec<Feature>, Vec<Feature>)> {
        let field_features = self
            .field
            .attrs
            .iter()
            .filter_map(|attr| {
                if attr.path().is_ident("salvo") {
                    attribute::find_nested_list(attr, "parameter")
                        .ok()
                        .flatten()
                        .map(|metas| {
                            metas
                                .parse_args::<FieldFeatures>()
                                .map_err(Diagnostic::from)
                                .map(|m| m.into_inner())
                        })
                } else {
                    None
                }
            })
            .collect::<DiagResult<Vec<_>>>()?
            .into_iter()
            .reduce(|acc, item| acc.merge(item))
            .unwrap_or_default();

        Ok(field_features.into_iter().fold(
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
        ))
    }

    fn to_extract_field_token_stream(&self, salvo: &Ident) -> DiagResult<TokenStream> {
        let (_, mut param_features) = self.resolve_field_features()?;
        let name = self
            .field
            .ident
            .as_ref()
            .expect("struct field name should be exists")
            .to_string();

        let rename = param_features
            .pop_rename_feature()
            .map(|rename| rename.into_value());
        let rename = rename.map(|rename| quote!(.rename(#rename)));
        let serde_rename = self.field_serde_params.as_ref().map(|field_param_serde| {
            field_param_serde
                .rename
                .as_ref()
                .map(|rename| quote!(.serde_rename(#rename)))
        });
        if let Some(parameter_in) = param_features.pop_parameter_in_feature() {
            let source = match parameter_in {
                attributes::ParameterIn(crate::parameter::ParameterIn::Query) => {
                    quote! { #salvo::extract::metadata::Source::new(#salvo::extract::metadata::SourceFrom::Query, #salvo::extract::metadata::SourceParser::Smart) }
                }
                attributes::ParameterIn(crate::parameter::ParameterIn::Header) => {
                    quote! { #salvo::extract::metadata::Source::new(#salvo::extract::metadata::SourceFrom::Header, #salvo::extract::metadata::SourceParser::Smart) }
                }
                attributes::ParameterIn(crate::parameter::ParameterIn::Path) => {
                    quote! { #salvo::extract::metadata::Source::new(#salvo::extract::metadata::SourceFrom::Param, #salvo::extract::metadata::SourceParser::Smart) }
                }
                attributes::ParameterIn(crate::parameter::ParameterIn::Cookie) => {
                    quote! { #salvo::extract::metadata::Source::new(#salvo::extract::metadata::SourceFrom::Cookie, #salvo::extract::metadata::SourceParser::Smart) }
                }
            };
            Ok(quote! {
                #salvo::extract::metadata::Field::new(#name)
                    .add_source(#source)
                    #rename
                    #serde_rename
            })
        } else {
            Ok(quote! {
                #salvo::extract::metadata::Field::new(#name)
                #rename
                #serde_rename
            })
        }
    }
}

impl TryToTokens for Parameter<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let field = self.field;
        let ident = &field.ident;
        let mut name = &*ident
            .as_ref()
            .map(|ident| ident.to_string())
            .or_else(|| self.container_attributes.name.cloned())
            .ok_or_else(|| {
                Diagnostic::spanned(
                    field.span(), DiagLevel::Error, "No name specified for unnamed field.").
                    help ("Try adding #[salvo(parameters(names(...)))] container attribute to specify the name for this field"
                )
            })?;

        if name.starts_with("r#") {
            name = &name[2..];
        }

        let (schema_features, mut param_features) = self.resolve_field_features()?;

        let rename = param_features
            .pop_rename_feature()
            .map(|rename| Cow::Owned(rename.into_value()))
            .or_else(|| {
                self.field_serde_params
                    .as_ref()
                    .and_then(|field_param_serde| {
                        field_param_serde.rename.as_deref().map(Cow::Borrowed)
                    })
            });
        let rename_all = self
            .container_attributes
            .rename_all
            .map(|rename_all| rename_all.as_rename_rule())
            .or_else(|| {
                self.serde_container
                    .as_ref()
                    .and_then(|serde_container| serde_container.rename_all.as_ref())
            });
        let name =
            crate::rename::<FieldRename>(name, rename, rename_all).unwrap_or(Cow::Borrowed(name));
        let type_tree = TypeTree::from_type(&field.ty)?;

        tokens.extend(quote! { #oapi::oapi::parameter::Parameter::new(#name)});

        if let Some(parameter_in) = param_features.pop_parameter_in_feature() {
            tokens.extend(quote! { .parameter_in(#parameter_in) });
        } else if let Some(parameter_in) = &self.container_attributes.default_parameter_in {
            tokens.extend(parameter_in.try_to_token_stream()?);
        }

        if let Some(style) = param_features.pop_style_feature() {
            tokens.extend(quote! { .style(#style) });
        } else if let Some(style) = &self.container_attributes.default_style {
            tokens.extend(style.try_to_token_stream());
        }

        if let Some(deprecated) = crate::get_deprecated(&field.attrs) {
            tokens.extend(quote! { .deprecated(#deprecated) });
        }

        let schema_with = pop_feature!(param_features => Feature::SchemaWith(_))
            .map(|f| f.try_to_token_stream())
            .transpose()?;
        if let Some(schema_with) = schema_with {
            tokens.extend(quote! { .schema(#schema_with) });
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
                .transpose()?
                .unwrap_or(type_tree);

            let required: Option<attributes::Required> =
                pop_feature!(param_features => Feature::Required(_)).into_inner();
            let component_required = !component.is_option()
                && crate::is_required(self.field_serde_params.as_ref(), self.serde_container);

            let required = match (required, component_required) {
                (Some(required_feature), _) => Into::<Required>::into(required_feature.is_true()),
                (None, component_required) => Into::<Required>::into(component_required),
            };
            tokens.extend(quote! {
                .required(#required)
            });
            tokens.extend(param_features.try_to_token_stream()?);

            let schema = ComponentSchema::new(component::ComponentSchemaProps {
                type_tree: &component,
                features: Some(schema_features),
                description: None,
                deprecated: None,
                object_name: "",
            })?
            .to_token_stream();

            tokens.extend(quote! { .schema(#schema) });
        }
        Ok(())
    }
}
