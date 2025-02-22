use std::{fmt::Display, str::FromStr};

use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, quote};
use syn::{LitFloat, LitInt, parse::ParseStream};

mod ext;
pub(crate) use ext::*;
mod macros;
pub(crate) use macros::*;

pub(crate) mod attributes;
pub(crate) mod validation;
pub(crate) mod validators;

use crate::feature::attributes::*;
use crate::feature::validation::*;
use crate::feature::validators::*;
use crate::schema_type::SchemaType;
use crate::type_tree::TypeTree;
use crate::{DiagLevel, DiagResult, Diagnostic, IntoInner, TryToTokens, parse_utils};

/// Parse `LitInt` from parse stream
fn parse_integer<T: FromStr + Display>(input: ParseStream) -> syn::Result<T>
where
    <T as FromStr>::Err: Display,
{
    parse_utils::parse_next(input, || input.parse::<LitInt>()?.base10_parse())
}

/// Parse any `number`. Tries to parse `LitInt` or `LitFloat` from parse stream.
fn parse_number<T>(input: ParseStream) -> syn::Result<T>
where
    T: FromStr,
    <T as FromStr>::Err: Display,
{
    parse_utils::parse_next(input, || {
        let lookup = input.lookahead1();
        if lookup.peek(LitInt) {
            input.parse::<LitInt>()?.base10_parse()
        } else if lookup.peek(LitFloat) {
            input.parse::<LitFloat>()?.base10_parse()
        } else {
            Err(lookup.error())
        }
    })
}

pub(crate) trait GetName {
    fn get_name() -> &'static str
    where
        Self: Sized;
}

/// Define whether [`Feature`] variant is validatable or not
pub(crate) trait Validatable {
    fn is_validatable(&self) -> bool {
        false
    }
}

pub(crate) trait Validate: Validatable {
    /// Perform validation check against schema type.
    fn validate(&self, validator: impl Validator) -> Result<(), Diagnostic>;
}

pub(crate) trait Parse {
    fn parse(input: ParseStream, attribute: Ident) -> syn::Result<Self>
    where
        Self: std::marker::Sized;
}

#[derive(Clone, Debug)]
pub(crate) enum Feature {
    Example(Example),
    Examples(Examples),
    Default(Default),
    Inline(Inline),
    XmlAttr(XmlAttr),
    Format(Format),
    ValueType(ValueType),
    WriteOnly(WriteOnly),
    ReadOnly(ReadOnly),
    Name(Name),
    Title(Title),
    Aliases(Aliases),
    Nullable(Nullable),
    Rename(Rename),
    RenameAll(RenameAll),
    Style(Style),
    DefaultStyle(DefaultStyle),
    AllowReserved(AllowReserved),
    Explode(Explode),
    ParameterIn(ParameterIn),
    DefaultParameterIn(DefaultParameterIn),
    ToParametersNames(ToParametersNames),
    MultipleOf(MultipleOf),
    Maximum(Maximum),
    Minimum(Minimum),
    ExclusiveMaximum(ExclusiveMaximum),
    ExclusiveMinimum(ExclusiveMinimum),
    MaxLength(MaxLength),
    MinLength(MinLength),
    Pattern(Pattern),
    MaxItems(MaxItems),
    MinItems(MinItems),
    MaxProperties(MaxProperties),
    MinProperties(MinProperties),
    SchemaWith(SchemaWith),
    Description(Description),
    Deprecated(Deprecated),
    Skip(Skip),
    AdditionalProperties(AdditionalProperties),
    Required(Required),
    SkipBound(SkipBound),
    Bound(Bound),
    ContentEncoding(ContentEncoding),
    ContentMediaType(ContentMediaType),
}

impl Feature {
    pub(crate) fn validate(
        &self,
        schema_type: &SchemaType,
        type_tree: &TypeTree,
    ) -> DiagResult<()> {
        match self {
            Feature::MultipleOf(multiple_of) => multiple_of.validate(
                ValidatorChain::new(&IsNumber(schema_type)).next(&AboveZeroF64(multiple_of.0)),
            ),
            Feature::Maximum(maximum) => maximum.validate(IsNumber(schema_type)),
            Feature::Minimum(minimum) => minimum.validate(IsNumber(schema_type)),
            Feature::ExclusiveMaximum(exclusive_maximum) => {
                exclusive_maximum.validate(IsNumber(schema_type))
            }
            Feature::ExclusiveMinimum(exclusive_minimum) => {
                exclusive_minimum.validate(IsNumber(schema_type))
            }
            Feature::MaxLength(max_length) => max_length.validate(
                ValidatorChain::new(&IsString(schema_type)).next(&AboveZeroUsize(max_length.0)),
            ),
            Feature::MinLength(min_length) => min_length.validate(
                ValidatorChain::new(&IsString(schema_type)).next(&AboveZeroUsize(min_length.0)),
            ),
            Feature::Pattern(pattern) => pattern.validate(IsString(schema_type)),
            Feature::MaxItems(max_items) => max_items.validate(
                ValidatorChain::new(&AboveZeroUsize(max_items.0)).next(&IsVec(type_tree)),
            ),
            Feature::MinItems(min_items) => min_items.validate(
                ValidatorChain::new(&AboveZeroUsize(min_items.0)).next(&IsVec(type_tree)),
            ),
            _unsupported_variant => {
                const SUPPORTED_VARIANTS: [&str; 10] = [
                    "multiple_of",
                    "maximum",
                    "minimum",
                    "exclusive_<aximum",
                    "exclusive_minimum",
                    "max_length",
                    "min_length",
                    "pattern",
                    "max_items",
                    "min_items",
                ];
                panic!(
                    "Unsupported variant: `{variant}` for Validate::validate, expected one of: {variants}",
                    variant = _unsupported_variant,
                    variants = SUPPORTED_VARIANTS.join(", ")
                )
            }
        }
    }
}

impl TryToTokens for Feature {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let feature = match &self {
            Feature::Default(default) => {
                if let Some(default) = &default.0 {
                    quote! { .default_value(#default) }
                } else {
                    quote! {}
                }
            }
            Feature::Example(example) => quote! { .example(#example) },
            Feature::Examples(examples) => quote! { .examples(#examples) },
            Feature::XmlAttr(xml) => quote! { .xml(#xml) },
            Feature::Format(format) => {
                let format = format.try_to_token_stream()?;
                quote! { .format(#format) }
            }
            Feature::WriteOnly(write_only) => quote! { .write_only(#write_only) },
            Feature::ReadOnly(read_only) => quote! { .read_only(#read_only) },
            Feature::Name(name) => quote! { .name(#name) },
            Feature::Title(title) => quote! { .title(#title) },
            Feature::Aliases(_) => {
                return Err(Diagnostic::spanned(
                    Span::call_site(),
                    DiagLevel::Error,
                    "Aliases feature does not support `TryToTokens`",
                ));
            }
            Feature::Nullable(_) => {
                return Err(Diagnostic::spanned(
                    Span::call_site(),
                    DiagLevel::Error,
                    "Nullable does not support `TryToTokens`",
                ));
            }
            Feature::Required(required) => quote! { .required(#required) },
            Feature::Rename(rename) => rename.to_token_stream(),
            Feature::Style(style) => quote! { .style(#style) },
            Feature::DefaultStyle(style) => quote! { .style(#style) },
            Feature::ParameterIn(parameter_in) => quote! { .parameter_in(#parameter_in) },
            Feature::DefaultParameterIn(parameter_in) => quote! { .parameter_in(#parameter_in) },
            Feature::MultipleOf(multiple_of) => quote! { .multiple_of(#multiple_of) },
            Feature::AllowReserved(allow_reserved) => {
                quote! { .allow_reserved(Some(#allow_reserved)) }
            }
            Feature::Explode(explode) => quote! { .explode(#explode) },
            Feature::Maximum(maximum) => quote! { .maximum(#maximum) },
            Feature::Minimum(minimum) => quote! { .minimum(#minimum) },
            Feature::ExclusiveMaximum(exclusive_maximum) => {
                quote! { .exclusive_maximum(#exclusive_maximum) }
            }
            Feature::ExclusiveMinimum(exclusive_minimum) => {
                quote! { .exclusive_minimum(#exclusive_minimum) }
            }
            Feature::MaxLength(max_length) => quote! { .max_length(#max_length) },
            Feature::MinLength(min_length) => quote! { .min_length(#min_length) },
            Feature::Pattern(pattern) => quote! { .pattern(#pattern) },
            Feature::MaxItems(max_items) => quote! { .max_items(#max_items) },
            Feature::MinItems(min_items) => quote! { .min_items(#min_items) },
            Feature::MaxProperties(max_properties) => {
                quote! { .max_properties(#max_properties) }
            }
            Feature::MinProperties(min_properties) => {
                quote! { .max_properties(#min_properties) }
            }
            Feature::SchemaWith(with_schema) => with_schema.to_token_stream(),
            Feature::Description(description) => quote! { .description(#description) },
            Feature::Deprecated(deprecated) => quote! { .deprecated(#deprecated) },
            Feature::Skip(_) => TokenStream::new(),
            Feature::AdditionalProperties(additional_properties) => {
                quote! { .additional_properties(#additional_properties) }
            }
            Feature::ContentEncoding(content_encoding) => {
                quote! { .content_encoding(#content_encoding) }
            }
            Feature::ContentMediaType(content_media_type) => {
                quote! { .content_media_type(#content_media_type) }
            }
            Feature::RenameAll(_) => {
                return Err(Diagnostic::spanned(
                    Span::call_site(),
                    DiagLevel::Error,
                    "RenameAll feature does not support `TryToTokens`",
                ));
            }
            Feature::ValueType(_) => {
                return Err(Diagnostic::spanned(
                    Span::call_site(),
                    DiagLevel::Error,
                    "ValueType feature does not support `TryToTokens`",
                )
                .help(
                    "ValueType is supposed to be used with `TypeTree` in same manner as a resolved struct/field type.",
                ));
            }
            Feature::Inline(_) | Feature::SkipBound(_) | Feature::Bound(_) => {
                // inline， skip_bound and bound feature is ignored by `TryToTokens`
                TokenStream::new()
            }
            Feature::ToParametersNames(_) => {
                return Err(Diagnostic::spanned(
                    Span::call_site(),
                    DiagLevel::Error,
                    "Names feature does not support `TryToTokens`"
                ).help(
                    "Names is only used with ToParameters to artificially give names for unnamed struct type `ToParameters`."
                ));
            }
        };

        tokens.extend(feature);
        Ok(())
    }
}

impl Display for Feature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Feature::Default(default) => default.fmt(f),
            Feature::Example(example) => example.fmt(f),
            Feature::Examples(examples) => examples.fmt(f),
            Feature::XmlAttr(xml) => xml.fmt(f),
            Feature::Format(format) => format.fmt(f),
            Feature::WriteOnly(write_only) => write_only.fmt(f),
            Feature::ReadOnly(read_only) => read_only.fmt(f),
            Feature::Name(name) => name.fmt(f),
            Feature::Title(title) => title.fmt(f),
            Feature::Aliases(aliases) => aliases.fmt(f),
            Feature::Nullable(nullable) => nullable.fmt(f),
            Feature::Rename(rename) => rename.fmt(f),
            Feature::Style(style) => style.fmt(f),
            Feature::DefaultStyle(style) => style.fmt(f),
            Feature::ParameterIn(parameter_in) => parameter_in.fmt(f),
            Feature::DefaultParameterIn(parameter_in) => parameter_in.fmt(f),
            Feature::AllowReserved(allow_reserved) => allow_reserved.fmt(f),
            Feature::Explode(explode) => explode.fmt(f),
            Feature::RenameAll(rename_all) => rename_all.fmt(f),
            Feature::ValueType(value_type) => value_type.fmt(f),
            Feature::Inline(inline) => inline.fmt(f),
            Feature::ToParametersNames(names) => names.fmt(f),
            Feature::MultipleOf(multiple_of) => multiple_of.fmt(f),
            Feature::Maximum(maximum) => maximum.fmt(f),
            Feature::Minimum(minimum) => minimum.fmt(f),
            Feature::ExclusiveMaximum(exclusive_maximum) => exclusive_maximum.fmt(f),
            Feature::ExclusiveMinimum(exclusive_minimum) => exclusive_minimum.fmt(f),
            Feature::MaxLength(max_length) => max_length.fmt(f),
            Feature::MinLength(min_length) => min_length.fmt(f),
            Feature::Pattern(pattern) => pattern.fmt(f),
            Feature::MaxItems(max_items) => max_items.fmt(f),
            Feature::MinItems(min_items) => min_items.fmt(f),
            Feature::MaxProperties(max_properties) => max_properties.fmt(f),
            Feature::MinProperties(min_properties) => min_properties.fmt(f),
            Feature::SchemaWith(with_schema) => with_schema.fmt(f),
            Feature::Description(description) => description.fmt(f),
            Feature::Deprecated(deprecated) => deprecated.fmt(f),
            Feature::Skip(skip) => skip.fmt(f),
            Feature::AdditionalProperties(additional_properties) => additional_properties.fmt(f),
            Feature::Required(required) => required.fmt(f),
            Feature::SkipBound(skip) => skip.fmt(f),
            Feature::Bound(bound) => bound.fmt(f),
            Feature::ContentEncoding(content_encoding) => content_encoding.fmt(f),
            Feature::ContentMediaType(content_media_type) => content_media_type.fmt(f),
        }
    }
}

impl Validatable for Feature {
    fn is_validatable(&self) -> bool {
        match &self {
            Feature::Default(default) => default.is_validatable(),
            Feature::Example(example) => example.is_validatable(),
            Feature::Examples(examples) => examples.is_validatable(),
            Feature::XmlAttr(xml) => xml.is_validatable(),
            Feature::Format(format) => format.is_validatable(),
            Feature::WriteOnly(write_only) => write_only.is_validatable(),
            Feature::ReadOnly(read_only) => read_only.is_validatable(),
            Feature::Name(name) => name.is_validatable(),
            Feature::Title(title) => title.is_validatable(),
            Feature::Aliases(aliases) => aliases.is_validatable(),
            Feature::Nullable(nullable) => nullable.is_validatable(),
            Feature::Rename(rename) => rename.is_validatable(),
            Feature::Style(style) => style.is_validatable(),
            Feature::DefaultStyle(style) => style.is_validatable(),
            Feature::ParameterIn(parameter_in) => parameter_in.is_validatable(),
            Feature::DefaultParameterIn(parameter_in) => parameter_in.is_validatable(),
            Feature::AllowReserved(allow_reserved) => allow_reserved.is_validatable(),
            Feature::Explode(explode) => explode.is_validatable(),
            Feature::RenameAll(rename_all) => rename_all.is_validatable(),
            Feature::ValueType(value_type) => value_type.is_validatable(),
            Feature::Inline(inline) => inline.is_validatable(),
            Feature::ToParametersNames(names) => names.is_validatable(),
            Feature::MultipleOf(multiple_of) => multiple_of.is_validatable(),
            Feature::Maximum(maximum) => maximum.is_validatable(),
            Feature::Minimum(minimum) => minimum.is_validatable(),
            Feature::ExclusiveMaximum(exclusive_maximum) => exclusive_maximum.is_validatable(),
            Feature::ExclusiveMinimum(exclusive_minimum) => exclusive_minimum.is_validatable(),
            Feature::MaxLength(max_length) => max_length.is_validatable(),
            Feature::MinLength(min_length) => min_length.is_validatable(),
            Feature::Pattern(pattern) => pattern.is_validatable(),
            Feature::MaxItems(max_items) => max_items.is_validatable(),
            Feature::MinItems(min_items) => min_items.is_validatable(),
            Feature::MaxProperties(max_properties) => max_properties.is_validatable(),
            Feature::MinProperties(min_properties) => min_properties.is_validatable(),
            Feature::SchemaWith(with_schema) => with_schema.is_validatable(),
            Feature::Description(description) => description.is_validatable(),
            Feature::Deprecated(deprecated) => deprecated.is_validatable(),
            Feature::Skip(skip) => skip.is_validatable(),
            Feature::AdditionalProperties(additional_properites) => {
                additional_properites.is_validatable()
            }
            Feature::Required(required) => required.is_validatable(),
            Feature::SkipBound(skip) => skip.is_validatable(),
            Feature::Bound(bound) => bound.is_validatable(),
            Feature::ContentEncoding(content_encoding) => content_encoding.is_validatable(),
            Feature::ContentMediaType(content_media_type) => content_media_type.is_validatable(),
        }
    }
}

pub(crate) trait IsInline {
    fn is_inline(&self) -> bool;
}

impl IsInline for Vec<Feature> {
    fn is_inline(&self) -> bool {
        self.iter()
            .find_map(|feature| match feature {
                Feature::Inline(inline) if inline.0 => Some(inline),
                _ => None,
            })
            .is_some()
    }
}

pub(crate) trait IsSkipped {
    fn is_skipped(&self) -> bool;
}

impl IsSkipped for Vec<Feature> {
    fn is_skipped(&self) -> bool {
        self.iter()
            .find_map(|feature| match feature {
                Feature::Skip(skip) if skip.0 => Some(skip),
                _ => None,
            })
            .is_some()
    }
}

pub(crate) trait Merge<T>: IntoInner<Vec<Feature>> {
    fn merge(self, from: T) -> Self;
}

impl IntoInner<Vec<Feature>> for Vec<Feature> {
    fn into_inner(self) -> Vec<Feature> {
        self
    }
}

impl Merge<Vec<Feature>> for Vec<Feature> {
    fn merge(mut self, mut from: Vec<Feature>) -> Self {
        self.append(&mut from);
        self
    }
}
