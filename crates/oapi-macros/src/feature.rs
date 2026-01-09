use std::fmt::{self, Debug, Display, Formatter};
use std::str::FromStr;

use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, quote};
use syn::parse::ParseStream;
use syn::{LitFloat, LitInt};

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
            Self::MultipleOf(multiple_of) => multiple_of.validate(
                ValidatorChain::new(&IsNumber(schema_type)).next(&AboveZeroF64(multiple_of.0)),
            ),
            Self::Maximum(maximum) => maximum.validate(IsNumber(schema_type)),
            Self::Minimum(minimum) => minimum.validate(IsNumber(schema_type)),
            Self::ExclusiveMaximum(exclusive_maximum) => {
                exclusive_maximum.validate(IsNumber(schema_type))
            }
            Self::ExclusiveMinimum(exclusive_minimum) => {
                exclusive_minimum.validate(IsNumber(schema_type))
            }
            Self::MaxLength(max_length) => max_length.validate(
                ValidatorChain::new(&IsString(schema_type)).next(&AboveZeroUsize(max_length.0)),
            ),
            Self::MinLength(min_length) => min_length.validate(
                ValidatorChain::new(&IsString(schema_type)).next(&AboveZeroUsize(min_length.0)),
            ),
            Self::Pattern(pattern) => pattern.validate(IsString(schema_type)),
            Self::MaxItems(max_items) => max_items.validate(
                ValidatorChain::new(&AboveZeroUsize(max_items.0)).next(&IsVec(type_tree)),
            ),
            Self::MinItems(min_items) => min_items.validate(
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
            Self::Default(default) => {
                if let Some(default) = &default.0 {
                    quote! { .default_value(#default) }
                } else {
                    quote! {}
                }
            }
            Self::Example(example) => quote! { .example(#example) },
            Self::Examples(examples) => quote! { .examples(#examples) },
            Self::XmlAttr(xml) => quote! { .xml(#xml) },
            Self::Format(format) => {
                let format = format.try_to_token_stream()?;
                quote! { .format(#format) }
            }
            Self::WriteOnly(write_only) => quote! { .write_only(#write_only) },
            Self::ReadOnly(read_only) => quote! { .read_only(#read_only) },
            Self::Name(name) => quote! { .name(#name) },
            Self::Title(title) => quote! { .title(#title) },
            Self::Aliases(_) => {
                return Err(Diagnostic::spanned(
                    Span::call_site(),
                    DiagLevel::Error,
                    "Aliases feature does not support `TryToTokens`",
                ));
            }
            Self::Nullable(_) => {
                return Err(Diagnostic::spanned(
                    Span::call_site(),
                    DiagLevel::Error,
                    "Nullable does not support `TryToTokens`",
                ));
            }
            Self::Required(required) => quote! { .required(#required) },
            Self::Rename(rename) => rename.to_token_stream(),
            Self::Style(style) => quote! { .style(#style) },
            Self::DefaultStyle(style) => quote! { .style(#style) },
            Self::ParameterIn(parameter_in) => quote! { .parameter_in(#parameter_in) },
            Self::DefaultParameterIn(parameter_in) => quote! { .parameter_in(#parameter_in) },
            Self::MultipleOf(multiple_of) => quote! { .multiple_of(#multiple_of) },
            Self::AllowReserved(allow_reserved) => {
                quote! { .allow_reserved(Some(#allow_reserved)) }
            }
            Self::Explode(explode) => quote! { .explode(#explode) },
            Self::Maximum(maximum) => quote! { .maximum(#maximum) },
            Self::Minimum(minimum) => quote! { .minimum(#minimum) },
            Self::ExclusiveMaximum(exclusive_maximum) => {
                quote! { .exclusive_maximum(#exclusive_maximum) }
            }
            Self::ExclusiveMinimum(exclusive_minimum) => {
                quote! { .exclusive_minimum(#exclusive_minimum) }
            }
            Self::MaxLength(max_length) => quote! { .max_length(#max_length) },
            Self::MinLength(min_length) => quote! { .min_length(#min_length) },
            Self::Pattern(pattern) => quote! { .pattern(#pattern) },
            Self::MaxItems(max_items) => quote! { .max_items(#max_items) },
            Self::MinItems(min_items) => quote! { .min_items(#min_items) },
            Self::MaxProperties(max_properties) => {
                quote! { .max_properties(#max_properties) }
            }
            Self::MinProperties(min_properties) => {
                quote! { .max_properties(#min_properties) }
            }
            Self::SchemaWith(with_schema) => with_schema.to_token_stream(),
            Self::Description(description) => quote! { .description(#description) },
            Self::Deprecated(deprecated) => quote! { .deprecated(#deprecated) },
            Self::Skip(_) => TokenStream::new(),
            Self::AdditionalProperties(additional_properties) => {
                quote! { .additional_properties(#additional_properties) }
            }
            Self::ContentEncoding(content_encoding) => {
                quote! { .content_encoding(#content_encoding) }
            }
            Self::ContentMediaType(content_media_type) => {
                quote! { .content_media_type(#content_media_type) }
            }
            Self::RenameAll(_) => {
                return Err(Diagnostic::spanned(
                    Span::call_site(),
                    DiagLevel::Error,
                    "RenameAll feature does not support `TryToTokens`",
                ));
            }
            Self::ValueType(_) => {
                return Err(Diagnostic::spanned(
                    Span::call_site(),
                    DiagLevel::Error,
                    "ValueType feature does not support `TryToTokens`",
                )
                .help(
                    "ValueType is supposed to be used with `TypeTree` in same manner as a resolved struct/field type.",
                ));
            }
            Self::Inline(_) | Self::SkipBound(_) | Self::Bound(_) => {
                // inline， skip_bound and bound feature is ignored by `TryToTokens`
                TokenStream::new()
            }
            Self::ToParametersNames(_) => {
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
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default(default) => default.fmt(f),
            Self::Example(example) => example.fmt(f),
            Self::Examples(examples) => examples.fmt(f),
            Self::XmlAttr(xml) => xml.fmt(f),
            Self::Format(format) => format.fmt(f),
            Self::WriteOnly(write_only) => write_only.fmt(f),
            Self::ReadOnly(read_only) => read_only.fmt(f),
            Self::Name(name) => name.fmt(f),
            Self::Title(title) => title.fmt(f),
            Self::Aliases(aliases) => aliases.fmt(f),
            Self::Nullable(nullable) => nullable.fmt(f),
            Self::Rename(rename) => rename.fmt(f),
            Self::Style(style) => style.fmt(f),
            Self::DefaultStyle(style) => style.fmt(f),
            Self::ParameterIn(parameter_in) => parameter_in.fmt(f),
            Self::DefaultParameterIn(parameter_in) => parameter_in.fmt(f),
            Self::AllowReserved(allow_reserved) => allow_reserved.fmt(f),
            Self::Explode(explode) => explode.fmt(f),
            Self::RenameAll(rename_all) => rename_all.fmt(f),
            Self::ValueType(value_type) => value_type.fmt(f),
            Self::Inline(inline) => inline.fmt(f),
            Self::ToParametersNames(names) => names.fmt(f),
            Self::MultipleOf(multiple_of) => multiple_of.fmt(f),
            Self::Maximum(maximum) => maximum.fmt(f),
            Self::Minimum(minimum) => minimum.fmt(f),
            Self::ExclusiveMaximum(exclusive_maximum) => exclusive_maximum.fmt(f),
            Self::ExclusiveMinimum(exclusive_minimum) => exclusive_minimum.fmt(f),
            Self::MaxLength(max_length) => max_length.fmt(f),
            Self::MinLength(min_length) => min_length.fmt(f),
            Self::Pattern(pattern) => pattern.fmt(f),
            Self::MaxItems(max_items) => max_items.fmt(f),
            Self::MinItems(min_items) => min_items.fmt(f),
            Self::MaxProperties(max_properties) => max_properties.fmt(f),
            Self::MinProperties(min_properties) => min_properties.fmt(f),
            Self::SchemaWith(with_schema) => with_schema.fmt(f),
            Self::Description(description) => description.fmt(f),
            Self::Deprecated(deprecated) => deprecated.fmt(f),
            Self::Skip(skip) => skip.fmt(f),
            Self::AdditionalProperties(additional_properties) => additional_properties.fmt(f),
            Self::Required(required) => required.fmt(f),
            Self::SkipBound(skip) => skip.fmt(f),
            Self::Bound(bound) => bound.fmt(f),
            Self::ContentEncoding(content_encoding) => content_encoding.fmt(f),
            Self::ContentMediaType(content_media_type) => content_media_type.fmt(f),
        }
    }
}

impl Validatable for Feature {
    fn is_validatable(&self) -> bool {
        match &self {
            Self::Default(default) => default.is_validatable(),
            Self::Example(example) => example.is_validatable(),
            Self::Examples(examples) => examples.is_validatable(),
            Self::XmlAttr(xml) => xml.is_validatable(),
            Self::Format(format) => format.is_validatable(),
            Self::WriteOnly(write_only) => write_only.is_validatable(),
            Self::ReadOnly(read_only) => read_only.is_validatable(),
            Self::Name(name) => name.is_validatable(),
            Self::Title(title) => title.is_validatable(),
            Self::Aliases(aliases) => aliases.is_validatable(),
            Self::Nullable(nullable) => nullable.is_validatable(),
            Self::Rename(rename) => rename.is_validatable(),
            Self::Style(style) => style.is_validatable(),
            Self::DefaultStyle(style) => style.is_validatable(),
            Self::ParameterIn(parameter_in) => parameter_in.is_validatable(),
            Self::DefaultParameterIn(parameter_in) => parameter_in.is_validatable(),
            Self::AllowReserved(allow_reserved) => allow_reserved.is_validatable(),
            Self::Explode(explode) => explode.is_validatable(),
            Self::RenameAll(rename_all) => rename_all.is_validatable(),
            Self::ValueType(value_type) => value_type.is_validatable(),
            Self::Inline(inline) => inline.is_validatable(),
            Self::ToParametersNames(names) => names.is_validatable(),
            Self::MultipleOf(multiple_of) => multiple_of.is_validatable(),
            Self::Maximum(maximum) => maximum.is_validatable(),
            Self::Minimum(minimum) => minimum.is_validatable(),
            Self::ExclusiveMaximum(exclusive_maximum) => exclusive_maximum.is_validatable(),
            Self::ExclusiveMinimum(exclusive_minimum) => exclusive_minimum.is_validatable(),
            Self::MaxLength(max_length) => max_length.is_validatable(),
            Self::MinLength(min_length) => min_length.is_validatable(),
            Self::Pattern(pattern) => pattern.is_validatable(),
            Self::MaxItems(max_items) => max_items.is_validatable(),
            Self::MinItems(min_items) => min_items.is_validatable(),
            Self::MaxProperties(max_properties) => max_properties.is_validatable(),
            Self::MinProperties(min_properties) => min_properties.is_validatable(),
            Self::SchemaWith(with_schema) => with_schema.is_validatable(),
            Self::Description(description) => description.is_validatable(),
            Self::Deprecated(deprecated) => deprecated.is_validatable(),
            Self::Skip(skip) => skip.is_validatable(),
            Self::AdditionalProperties(additional_properties) => {
                additional_properties.is_validatable()
            }
            Self::Required(required) => required.is_validatable(),
            Self::SkipBound(skip) => skip.is_validatable(),
            Self::Bound(bound) => bound.is_validatable(),
            Self::ContentEncoding(content_encoding) => content_encoding.is_validatable(),
            Self::ContentMediaType(content_media_type) => content_media_type.is_validatable(),
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

impl IntoInner<Self> for Vec<Feature> {
    fn into_inner(self) -> Self {
        self
    }
}

impl Merge<Self> for Vec<Feature> {
    fn merge(mut self, mut from: Self) -> Self {
        self.append(&mut from);
        self
    }
}
