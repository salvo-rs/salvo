use std::{borrow::Cow, fmt::Display};

use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, quote};
use syn::parenthesized;
use syn::parse::{Parse, ParseBuffer, ParseStream};
use syn::{DeriveInput, Error, ExprPath, LitStr, Token};

mod derive;
use derive::ToParameters;

use crate::component::{self, ComponentSchema};
use crate::feature::attributes::{
    AllowReserved, Deprecated, Description, Example, Explode, Format, Nullable, ReadOnly, Style,
    WriteOnly, XmlAttr,
};
use crate::feature::validation::{
    ExclusiveMaximum, ExclusiveMinimum, MaxItems, MaxLength, Maximum, MinItems, MinLength, Minimum,
    MultipleOf, Pattern,
};
use crate::feature::{Feature, TryToTokensExt, parse_features};
use crate::{DiagLevel, DiagResult, Diagnostic, TryToTokens};
use crate::{Required, operation::InlineType, parse_utils};

pub(crate) fn to_parameters(input: DeriveInput) -> DiagResult<TokenStream> {
    let DeriveInput {
        attrs,
        ident,
        data,
        generics,
        ..
    } = input;
    ToParameters {
        attrs,
        generics,
        data,
        ident,
    }
    .try_to_token_stream()
}

/// Parameter of request such as in path, header, query or cookie
///
/// For example path `/users/{id}` the path parameter is used to define
/// type, format and other details of the `{id}` parameter within the path
///
/// Parse is executed for following formats:
///
/// * ("id" = String, Path, deprecated, description = "Users database id"),
/// * ("id", Path, deprecated, description = "Users database id"),
///
/// The `= String` type statement is optional if automatic resolution is supported.
#[derive(Debug)]
pub(crate) enum Parameter<'a> {
    Value(ValueParameter<'a>),
    /// Identifier for a struct that implements `ToParameters` trait.
    Struct(StructParameter),
}

impl Parse for Parameter<'_> {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.fork().parse::<ExprPath>().is_ok() {
            Ok(Self::Struct(StructParameter {
                path: input.parse()?,
            }))
        } else {
            Ok(Self::Value(input.parse()?))
        }
    }
}

impl TryToTokens for Parameter<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        match self {
            Parameter::Value(parameter) => {
                if parameter.parameter_in.is_none() {
                    let parameter = parameter.try_to_token_stream()?;
                    tokens.extend(quote!{
                        {
                            let mut new_parameter = #parameter;
                            if let Some(exist_parameter) = operation.parameters.0.iter_mut().find(|p|p.name == new_parameter.name) {
                                new_parameter.parameter_in = exist_parameter.parameter_in.clone();
                                exist_parameter.merge(new_parameter);
                            } else {
                                operation.parameters.insert(new_parameter);
                            }
                        }
                    });
                } else {
                    let parameter = parameter.try_to_token_stream()?;
                    tokens.extend(quote! { operation.parameters.insert(#parameter); })
                }
            }
            Parameter::Struct(StructParameter { path }) => tokens.extend(quote! {
                operation.parameters.extend(
                    <#path as #oapi::oapi::ToParameters>::to_parameters(components)
                );
            }),
        }
        Ok(())
    }
}

#[derive(Debug)]
struct ParameterSchema<'p> {
    parameter_type: ParameterType<'p>,
    features: Vec<Feature>,
}

impl TryToTokens for ParameterSchema<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let mut to_tokens = |param_schema, required| {
            tokens.extend(quote! { .schema(#param_schema).required(#required) });
        };

        match &self.parameter_type {
            ParameterType::Parsed(inline_type) => {
                let type_tree = inline_type.as_type_tree()?;
                let required: Required = (!type_tree.is_option()).into();
                let mut schema_features = Vec::<Feature>::new();
                schema_features.clone_from(&self.features);
                schema_features.push(Feature::Inline(inline_type.is_inline.into()));

                to_tokens(
                    ComponentSchema::new(component::ComponentSchemaProps {
                        type_tree: &type_tree,
                        features: Some(schema_features),
                        description: None,
                        deprecated: None,
                        object_name: "",
                    })?,
                    required,
                )
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
enum ParameterType<'p> {
    Parsed(InlineType<'p>),
}

#[derive(Default, Debug)]
pub(crate) struct ValueParameter<'a> {
    pub(crate) name: Cow<'a, str>,
    parameter_in: Option<ParameterIn>,
    parameter_schema: Option<ParameterSchema<'a>>,
    features: (Vec<Feature>, Vec<Feature>),
}

impl Parse for ValueParameter<'_> {
    fn parse(input_with_parens: ParseStream) -> syn::Result<Self> {
        let input: ParseBuffer;
        parenthesized!(input in input_with_parens);

        let mut parameter = ValueParameter::default();

        if input.peek(LitStr) {
            // parse name
            let name = input.parse::<LitStr>()?.value();
            parameter.name = Cow::Owned(name);

            if input.peek(Token![=]) {
                parameter.parameter_schema = Some(ParameterSchema {
                    parameter_type: ParameterType::Parsed(parse_utils::parse_next(&input, || {
                        input.parse().map_err(|error| {
                            Error::new(
                                error.span(),
                                format!("unexpected token, expected type such as String, {error}"),
                            )
                        })
                    })?),
                    features: Vec::new(),
                });
            }
        } else {
            return Err(input.error("unparsable parameter name, expected literal string"));
        }

        input.parse::<Token![,]>()?;

        if input.fork().parse::<ParameterIn>().is_ok() {
            parameter.parameter_in = Some(input.parse()?);
            let _ = input.parse::<Token![,]>();
        }

        let (schema_features, parameter_features) = input
            .parse::<ParameterFeatures>()?
            .split_for_parameter_type();

        parameter.features = (schema_features.clone(), parameter_features);
        if let Some(parameter_schema) = &mut parameter.parameter_schema {
            parameter_schema.features = schema_features;
        }

        Ok(parameter)
    }
}

#[derive(Default, Debug)]
struct ParameterFeatures(Vec<Feature>);

impl Parse for ParameterFeatures {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self(parse_features!(
            // parameter features
            input as Style,
            Explode,
            AllowReserved,
            Example,
            Deprecated,
            Description,
            // param schema features
            Format,
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
            MinItems
        )))
    }
}

impl ParameterFeatures {
    /// Split parsed features to two `Vec`s of [`Feature`]s.
    ///
    /// * First vec contains parameter type schema features.
    /// * Second vec contains generic parameter features.
    fn split_for_parameter_type(self) -> (Vec<Feature>, Vec<Feature>) {
        self.0.into_iter().fold(
            (Vec::new(), Vec::new()),
            |(mut schema_features, mut param_features), feature| {
                match feature {
                    Feature::Format(_)
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
                    | Feature::MinItems(_) => {
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

// impl_into_inner!(ParameterFeatures);

impl TryToTokens for ValueParameter<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let name = &*self.name;
        tokens.extend(quote! {
            #oapi::oapi::parameter::Parameter::from(#oapi::oapi::parameter::Parameter::new(#name))
        });
        if let Some(parameter_in) = &self.parameter_in {
            tokens.extend(quote! { .parameter_in(#parameter_in) });
        }

        let (schema_features, param_features) = &self.features;
        tokens.extend(param_features.try_to_token_stream()?);
        if !schema_features.is_empty() && self.parameter_schema.is_none() {
            return  Err(Diagnostic::new(
               DiagLevel::Error,
                "Missing `parameter_type` attribute, cannot define schema features without it.").help(
                    "See docs for more details <https://docs.rs/salvo_oapi/latest/salvo_oapi/attr.path.html#parameter-type-attributes>"
                )
            );
        }

        if let Some(parameter_schema) = &self.parameter_schema {
            parameter_schema.try_to_tokens(tokens)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct StructParameter {
    pub(crate) path: ExprPath,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub(crate) enum ParameterIn {
    Query,
    Path,
    Header,
    Cookie,
}

impl ParameterIn {
    pub(crate) const VARIANTS: &'static [Self] =
        &[Self::Query, Self::Path, Self::Header, Self::Cookie];
}

impl Display for ParameterIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParameterIn::Query => write!(f, "Query"),
            ParameterIn::Path => write!(f, "Path"),
            ParameterIn::Header => write!(f, "Header"),
            ParameterIn::Cookie => write!(f, "Cookie"),
        }
    }
}

impl Parse for ParameterIn {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        fn expected_style() -> String {
            let variants: String = ParameterIn::VARIANTS
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("unexpected in, expected one of: {variants}")
        }
        let style = parse_utils::parse_path_or_lit_str(input)?.to_lowercase();
        match &*style {
            "path" => Ok(Self::Path),
            "query" => Ok(Self::Query),
            "header" => Ok(Self::Header),
            "cookie" => Ok(Self::Cookie),
            _ => Err(Error::new(input.span(), expected_style())),
        }
    }
}

impl ToTokens for ParameterIn {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        tokens.extend(match self {
            Self::Path => quote! { #oapi::oapi::parameter::ParameterIn::Path },
            Self::Query => quote! { #oapi::oapi::parameter::ParameterIn::Query },
            Self::Header => quote! { #oapi::oapi::parameter::ParameterIn::Header },
            Self::Cookie => quote! { #oapi::oapi::parameter::ParameterIn::Cookie },
        })
    }
}

/// See definitions from `salvo_oapi` crate path.rs
#[derive(Copy, Clone, Debug)]
pub(crate) enum ParameterStyle {
    Matrix,
    Label,
    Form,
    Simple,
    SpaceDelimited,
    PipeDelimited,
    DeepObject,
}

impl Parse for ParameterStyle {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        const EXPECTED_STYLE: &str = "unexpected style, expected one of: Matrix, Label, Form, Simple, SpaceDelimited, PipeDelimited, DeepObject";
        let style = input.parse::<Ident>()?;

        match &*style.to_string() {
            "Matrix" => Ok(ParameterStyle::Matrix),
            "Label" => Ok(ParameterStyle::Label),
            "Form" => Ok(ParameterStyle::Form),
            "Simple" => Ok(ParameterStyle::Simple),
            "SpaceDelimited" => Ok(ParameterStyle::SpaceDelimited),
            "PipeDelimited" => Ok(ParameterStyle::PipeDelimited),
            "DeepObject" => Ok(ParameterStyle::DeepObject),
            _ => Err(Error::new(style.span(), EXPECTED_STYLE)),
        }
    }
}

impl ToTokens for ParameterStyle {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let oapi = crate::oapi_crate();
        match self {
            ParameterStyle::Matrix => {
                tokens.extend(quote! { #oapi::oapi::parameter::ParameterStyle::Matrix })
            }
            ParameterStyle::Label => {
                tokens.extend(quote! { #oapi::oapi::parameter::ParameterStyle::Label })
            }
            ParameterStyle::Form => {
                tokens.extend(quote! { #oapi::oapi::parameter::ParameterStyle::Form })
            }
            ParameterStyle::Simple => {
                tokens.extend(quote! { #oapi::oapi::parameter::ParameterStyle::Simple })
            }
            ParameterStyle::SpaceDelimited => {
                tokens.extend(quote! { #oapi::oapi::parameter::ParameterStyle::SpaceDelimited })
            }
            ParameterStyle::PipeDelimited => {
                tokens.extend(quote! { #oapi::oapi::parameter::ParameterStyle::PipeDelimited })
            }
            ParameterStyle::DeepObject => {
                tokens.extend(quote! { #oapi::oapi::parameter::ParameterStyle::DeepObject })
            }
        }
    }
}
