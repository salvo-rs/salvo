use std::{fmt::Display, mem};

use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens};
use syn::parse::ParseStream;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{parenthesized, token, LitStr, Token, Type, TypePath, WherePredicate};

use super::{impl_get_name, parse_integer, parse_number, Feature, Parse, Validate, Validator};
use crate::{
    parameter::{self, ParameterStyle},
    parse_utils, schema,
    schema_type::SchemaFormat,
    serde_util::RenameRule,
    type_tree::{GenericType, TypeTree},
    AnyValue, DiagLevel, DiagResult, Diagnostic, TryToTokens,
};

#[derive(Clone, Debug)]
pub(crate) struct Example(AnyValue);

impl Parse for Example {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_next(input, || AnyValue::parse_any(input)).map(Self)
    }
}

impl ToTokens for Example {
    fn to_tokens(&self, stream: &mut TokenStream) {
        stream.extend(self.0.to_token_stream())
    }
}

impl From<Example> for Feature {
    fn from(value: Example) -> Self {
        Feature::Example(value)
    }
}
impl_get_name!(Example = "example");

#[derive(Clone, Debug)]
pub(crate) struct Default(pub(crate) Option<AnyValue>);
impl Default {
    pub(crate) fn new_default_trait(struct_ident: Ident, field_ident: syn::Member) -> Self {
        Self(Some(AnyValue::new_default_trait(struct_ident, field_ident)))
    }
}
impl Parse for Default {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        if input.peek(syn::Token![=]) {
            parse_utils::parse_next(input, || AnyValue::parse_any(input)).map(|any| Self(Some(any)))
        } else {
            Ok(Self(None))
        }
    }
}
impl ToTokens for Default {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        match &self.0 {
            Some(inner) => stream.extend(quote! {Some(#inner)}),
            None => stream.extend(quote! {None}),
        }
    }
}
impl From<self::Default> for Feature {
    fn from(value: self::Default) -> Self {
        Feature::Default(value)
    }
}
impl_get_name!(Default = "default");

#[derive(Clone, Debug)]
pub(crate) struct Inline(pub(crate) bool);
impl Parse for Inline {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_bool_or_true(input).map(Self)
    }
}
impl From<bool> for Inline {
    fn from(value: bool) -> Self {
        Inline(value)
    }
}
impl From<Inline> for Feature {
    fn from(value: Inline) -> Self {
        Feature::Inline(value)
    }
}
impl_get_name!(Inline = "inline");

#[derive(Default, Clone, Debug)]
pub(crate) struct XmlAttr(pub(crate) schema::XmlAttr);
impl XmlAttr {
    /// Split [`XmlAttr`] for [`GenericType::Vec`] returning tuple of [`XmlAttr`]s where first
    /// one is for a vec and second one is for object field.
    pub(crate) fn split_for_vec(&mut self, type_tree: &TypeTree) -> DiagResult<(Option<XmlAttr>, Option<XmlAttr>)> {
        if matches!(type_tree.generic_type, Some(GenericType::Vec)) {
            let mut value_xml = mem::take(self);
            let vec_xml = schema::XmlAttr::with_wrapped(
                mem::take(&mut value_xml.0.is_wrapped),
                mem::take(&mut value_xml.0.wrap_name),
            );

            Ok((Some(XmlAttr(vec_xml)), Some(value_xml)))
        } else {
            self.validate_xml(&self.0)?;

            Ok((None, Some(mem::take(self))))
        }
    }

    #[inline]
    fn validate_xml(&self, xml: &schema::XmlAttr) -> DiagResult<()> {
        if let Some(wrapped_ident) = xml.is_wrapped.as_ref() {
            Err(Diagnostic::spanned(
                wrapped_ident.span(),
                DiagLevel::Error,
                "cannot use `wrapped` attribute in non slice field type",
            )
            .help("Try removing `wrapped` attribute or make your field `Vec`"))
        } else {
            Ok(())
        }
    }
}
impl Parse for XmlAttr {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        let xml;
        parenthesized!(xml in input);
        xml.parse::<schema::XmlAttr>().map(Self)
    }
}
impl ToTokens for XmlAttr {
    fn to_tokens(&self, stream: &mut TokenStream) {
        stream.extend(self.0.to_token_stream())
    }
}
impl From<XmlAttr> for Feature {
    fn from(value: XmlAttr) -> Self {
        Feature::XmlAttr(value)
    }
}
impl_get_name!(XmlAttr = "xml");

#[derive(Clone, Debug)]
pub(crate) struct Format(pub(crate) SchemaFormat<'static>);
impl Parse for Format {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_next(input, || input.parse::<SchemaFormat>()).map(Self)
    }
}
impl TryToTokens for Format {
    fn try_to_tokens(&self, stream: &mut TokenStream) -> DiagResult<()> {
        stream.extend(self.0.try_to_token_stream()?);
        Ok(())
    }
}
impl From<Format> for Feature {
    fn from(value: Format) -> Self {
        Feature::Format(value)
    }
}
impl_get_name!(Format = "format");

#[derive(Clone, Debug)]
pub(crate) struct ValueType(pub(crate) syn::Type);
impl ValueType {
    /// Create [`TypeTree`] from current [`syn::Type`].
    pub(crate) fn as_type_tree(&self) -> DiagResult<TypeTree> {
        TypeTree::from_type(&self.0)
    }
}
impl Parse for ValueType {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_next(input, || input.parse::<syn::Type>()).map(Self)
    }
}
impl From<ValueType> for Feature {
    fn from(value: ValueType) -> Self {
        Feature::ValueType(value)
    }
}
impl_get_name!(ValueType = "value_type");

#[derive(Clone, Copy, Debug)]
pub(crate) struct WriteOnly(pub(crate) bool);
impl Parse for WriteOnly {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_bool_or_true(input).map(Self)
    }
}
impl ToTokens for WriteOnly {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        stream.extend(self.0.to_token_stream())
    }
}
impl From<WriteOnly> for Feature {
    fn from(value: WriteOnly) -> Self {
        Feature::WriteOnly(value)
    }
}
impl_get_name!(WriteOnly = "write_only");

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReadOnly(pub(crate) bool);
impl Parse for ReadOnly {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_bool_or_true(input).map(Self)
    }
}
impl ToTokens for ReadOnly {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        stream.extend(self.0.to_token_stream())
    }
}
impl From<ReadOnly> for Feature {
    fn from(value: ReadOnly) -> Self {
        Feature::ReadOnly(value)
    }
}
impl_get_name!(ReadOnly = "read_only");

#[derive(Clone, Debug)]
pub(crate) struct Name(pub(crate) String);
impl Parse for Name {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_next_path_or_lit_str(input).map(Self)
    }
}
impl ToTokens for Name {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        stream.extend(self.0.to_token_stream())
    }
}
impl From<Name> for Feature {
    fn from(value: Name) -> Self {
        Feature::Name(value)
    }
}
impl_get_name!(Name = "name");

#[derive(Clone, Debug)]
pub(crate) struct Title(pub(crate) String);
impl Parse for Title {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_next_lit_str(input).map(Self)
    }
}
impl ToTokens for Title {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        stream.extend(self.0.to_token_stream())
    }
}
impl From<Title> for Feature {
    fn from(value: Title) -> Self {
        Feature::Title(value)
    }
}
impl_get_name!(Title = "title");

#[derive(Clone, Copy, Debug)]
pub(crate) struct Nullable(pub(crate) bool);
impl Nullable {
    pub(crate) fn new() -> Self {
        Self(true)
    }
}
impl Parse for Nullable {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_bool_or_true(input).map(Self)
    }
}
impl ToTokens for Nullable {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        stream.extend(self.0.to_token_stream())
    }
}
impl From<Nullable> for Feature {
    fn from(value: Nullable) -> Self {
        Feature::Nullable(value)
    }
}
impl_get_name!(Nullable = "nullable");

#[derive(Clone, Debug)]
pub(crate) struct Rename(pub(crate) String);
impl Rename {
    pub(crate) fn into_value(self) -> String {
        self.0
    }
}
impl Parse for Rename {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_next_path_or_lit_str(input).map(Self)
    }
}
impl ToTokens for Rename {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        stream.extend(self.0.to_token_stream())
    }
}

impl From<Rename> for Feature {
    fn from(value: Rename) -> Self {
        Feature::Rename(value)
    }
}
impl_get_name!(Rename = "rename");

#[derive(Clone, Debug)]
pub(crate) struct RenameAll(pub(crate) RenameRule);
impl RenameAll {
    pub(crate) fn as_rename_rule(&self) -> &RenameRule {
        &self.0
    }
}
impl Parse for RenameAll {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        let litstr = parse_utils::parse_next(input, || input.parse::<LitStr>())?;

        litstr
            .value()
            .parse::<RenameRule>()
            .map_err(|error| syn::Error::new(litstr.span(), error.to_string()))
            .map(Self)
    }
}
impl From<RenameAll> for Feature {
    fn from(value: RenameAll) -> Self {
        Feature::RenameAll(value)
    }
}
impl_get_name!(RenameAll = "rename_all");

#[derive(Clone, Debug)]
pub(crate) struct DefaultStyle(pub(crate) ParameterStyle);
impl From<ParameterStyle> for DefaultStyle {
    fn from(style: ParameterStyle) -> Self {
        Self(style)
    }
}
impl Parse for DefaultStyle {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_next(input, || input.parse::<ParameterStyle>().map(Self))
    }
}
impl ToTokens for DefaultStyle {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream)
    }
}
impl From<DefaultStyle> for Feature {
    fn from(value: DefaultStyle) -> Self {
        Feature::DefaultStyle(value)
    }
}
impl_get_name!(DefaultStyle = "default_style");

#[derive(Clone, Debug)]
pub(crate) struct Style(pub(crate) ParameterStyle);
impl From<ParameterStyle> for Style {
    fn from(style: ParameterStyle) -> Self {
        Self(style)
    }
}
impl Parse for Style {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_next(input, || input.parse::<ParameterStyle>().map(Self))
    }
}
impl ToTokens for Style {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream)
    }
}
impl From<Style> for Feature {
    fn from(value: Style) -> Self {
        Feature::Style(value)
    }
}
impl_get_name!(Style = "style");

#[derive(Clone, Debug)]
pub(crate) struct AllowReserved(pub(crate) bool);
impl Parse for AllowReserved {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_bool_or_true(input).map(Self)
    }
}
impl ToTokens for AllowReserved {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream)
    }
}
impl From<AllowReserved> for Feature {
    fn from(value: AllowReserved) -> Self {
        Feature::AllowReserved(value)
    }
}
impl_get_name!(AllowReserved = "allow_reserved");

#[derive(Clone, Debug)]
pub(crate) struct Explode(pub(crate) bool);
impl Parse for Explode {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_bool_or_true(input).map(Self)
    }
}
impl ToTokens for Explode {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream)
    }
}
impl From<Explode> for Feature {
    fn from(value: Explode) -> Self {
        Feature::Explode(value)
    }
}
impl_get_name!(Explode = "explode");

#[derive(Clone, Debug)]
pub(crate) struct DefaultParameterIn(pub(crate) parameter::ParameterIn);
impl Parse for DefaultParameterIn {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_next(input, || input.parse::<parameter::ParameterIn>().map(Self))
    }
}
impl ToTokens for DefaultParameterIn {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<DefaultParameterIn> for Feature {
    fn from(value: DefaultParameterIn) -> Self {
        Feature::DefaultParameterIn(value)
    }
}
impl_get_name!(DefaultParameterIn = "default_parameter_in");

#[derive(Clone, Debug)]
pub(crate) struct ParameterIn(pub(crate) parameter::ParameterIn);
impl Parse for ParameterIn {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_next(input, || input.parse::<parameter::ParameterIn>().map(Self))
    }
}
impl ToTokens for ParameterIn {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<ParameterIn> for Feature {
    fn from(value: ParameterIn) -> Self {
        Feature::ParameterIn(value)
    }
}
impl_get_name!(ParameterIn = "parameter_in");

/// Specify names of unnamed fields with `names(...) attribute for `ToParameters` derive.
#[derive(Clone, Debug)]
pub(crate) struct ToParametersNames(pub(crate) Vec<String>);
impl ToParametersNames {
    pub(crate) fn into_values(self) -> Vec<String> {
        self.0
    }
}
impl Parse for ToParametersNames {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        Ok(Self(
            parse_utils::parse_punctuated_within_parenthesis::<LitStr>(input)?
                .iter()
                .map(LitStr::value)
                .collect(),
        ))
    }
}
impl From<ToParametersNames> for Feature {
    fn from(value: ToParametersNames) -> Self {
        Feature::ToParametersNames(value)
    }
}
impl_get_name!(ToParametersNames = "names");

#[derive(Clone, Debug)]
pub(crate) struct MultipleOf(pub(crate) f64, pub(crate) Ident);
impl Validate for MultipleOf {
    fn validate(&self, validator: impl Validator) -> DiagResult<()> {
        if let Err(error) = validator.is_valid() {
            Err(Diagnostic::spanned(
                self.1.span(),
                DiagLevel::Error,
                format!("`multiple_of` error: {}", error),
            )
            .help(
                "See more details: `http://json-schema.org/draft/2020-12/json-schema-validation.html#name-multipleof`",
            ))
        } else {
            Ok(())
        }
    }
}
impl Parse for MultipleOf {
    fn parse(input: ParseStream, ident: Ident) -> syn::Result<Self> {
        parse_number(input).map(|multiple_of| Self(multiple_of, ident))
    }
}
impl ToTokens for MultipleOf {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<MultipleOf> for Feature {
    fn from(value: MultipleOf) -> Self {
        Feature::MultipleOf(value)
    }
}
impl_get_name!(MultipleOf = "multiple_of");

#[derive(Clone, Debug)]
pub(crate) struct Maximum(pub(crate) f64, pub(crate) Ident);
impl Validate for Maximum {
    fn validate(&self, validator: impl Validator) -> DiagResult<()> {
        if let Err(error) = validator.is_valid() {
            Err(
                Diagnostic::spanned(self.1.span(), DiagLevel::Error, format!("`maximum` error: {}", error)).help(
                    "See more details: `http://json-schema.org/draft/2020-12/json-schema-validation.html#name-maximum`",
                ),
            )
        } else {
            Ok(())
        }
    }
}
impl Parse for Maximum {
    fn parse(input: ParseStream, ident: Ident) -> syn::Result<Self>
    where
        Self: Sized,
    {
        parse_number(input).map(|maximum| Self(maximum, ident))
    }
}
impl ToTokens for Maximum {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<Maximum> for Feature {
    fn from(value: Maximum) -> Self {
        Feature::Maximum(value)
    }
}
impl_get_name!(Maximum = "maximum");

#[derive(Clone, Debug)]
pub(crate) struct Minimum(pub(crate) f64, pub(crate) Ident);
impl Minimum {
    pub(crate) fn new(value: f64, span: Span) -> Self {
        Self(value, Ident::new("empty", span))
    }
}
impl Validate for Minimum {
    fn validate(&self, validator: impl Validator) -> DiagResult<()> {
        if let Err(error) = validator.is_valid() {
            Err(
                Diagnostic::spanned(self.1.span(), DiagLevel::Error, format!("`minimum` error: {}", error)).help(
                    "See more details: `http://json-schema.org/draft/2020-12/json-schema-validation.html#name-minimum`",
                ),
            )
        } else {
            Ok(())
        }
    }
}
impl Parse for Minimum {
    fn parse(input: ParseStream, ident: Ident) -> syn::Result<Self>
    where
        Self: Sized,
    {
        parse_number(input).map(|maximum| Self(maximum, ident))
    }
}
impl ToTokens for Minimum {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<Minimum> for Feature {
    fn from(value: Minimum) -> Self {
        Feature::Minimum(value)
    }
}
impl_get_name!(Minimum = "minimum");

#[derive(Clone, Debug)]
pub(crate) struct ExclusiveMaximum(pub(crate) f64, pub(crate) Ident);
impl Validate for ExclusiveMaximum {
    fn validate(&self, validator: impl Validator) -> DiagResult<()> {
        if let Err(error) = validator.is_valid() {
            Err(Diagnostic::spanned(self.1.span(), DiagLevel::Error, format!("`exclusive_maximum` error: {}", error))
            .help("See more details: `http://json-schema.org/draft/2020-12/json-schema-validation.html#name-exclusivemaximum`"))
        } else {
            Ok(())
        }
    }
}
impl Parse for ExclusiveMaximum {
    fn parse(input: ParseStream, ident: Ident) -> syn::Result<Self>
    where
        Self: Sized,
    {
        parse_number(input).map(|max| Self(max, ident))
    }
}
impl ToTokens for ExclusiveMaximum {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<ExclusiveMaximum> for Feature {
    fn from(value: ExclusiveMaximum) -> Self {
        Feature::ExclusiveMaximum(value)
    }
}
impl_get_name!(ExclusiveMaximum = "exclusive_maximum");

#[derive(Clone, Debug)]
pub(crate) struct ExclusiveMinimum(pub(crate) f64, pub(crate) Ident);
impl Validate for ExclusiveMinimum {
    fn validate(&self, validator: impl Validator) -> DiagResult<()> {
        if let Err(error) = validator.is_valid() {
            Err(Diagnostic::spanned(self.1.span(), DiagLevel::Error, format!("`exclusive_minimum` error: {}", error))
            .help("See more details: `http://json-schema.org/draft/2020-12/json-schema-validation.html#name-exclusiveminimum`"))
        } else {
            Ok(())
        }
    }
}
impl Parse for ExclusiveMinimum {
    fn parse(input: ParseStream, ident: Ident) -> syn::Result<Self>
    where
        Self: Sized,
    {
        parse_number(input).map(|min| Self(min, ident))
    }
}
impl ToTokens for ExclusiveMinimum {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<ExclusiveMinimum> for Feature {
    fn from(value: ExclusiveMinimum) -> Self {
        Feature::ExclusiveMinimum(value)
    }
}
impl_get_name!(ExclusiveMinimum = "exclusive_minimum");

#[derive(Clone, Debug)]
pub(crate) struct MaxLength(pub(crate) usize, pub(crate) Ident);
impl Validate for MaxLength {
    fn validate(&self, validator: impl Validator) -> DiagResult<()> {
        if let Err(error) = validator.is_valid() {
            Err(Diagnostic::spanned(
                self.1.span(),
                DiagLevel::Error,
                format!("`max_length` error: {}", error),
            )
            .help(
                "See more details: `http://json-schema.org/draft/2020-12/json-schema-validation.html#name-maxlength`",
            ))
        } else {
            Ok(())
        }
    }
}
impl Parse for MaxLength {
    fn parse(input: ParseStream, ident: Ident) -> syn::Result<Self>
    where
        Self: Sized,
    {
        parse_integer(input).map(|max_length| Self(max_length, ident))
    }
}
impl ToTokens for MaxLength {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<MaxLength> for Feature {
    fn from(value: MaxLength) -> Self {
        Feature::MaxLength(value)
    }
}
impl_get_name!(MaxLength = "max_length");

#[derive(Clone, Debug)]
pub(crate) struct MinLength(pub(crate) usize, pub(crate) Ident);
impl Validate for MinLength {
    fn validate(&self, validator: impl Validator) -> DiagResult<()> {
        if let Err(error) = validator.is_valid() {
            Err(Diagnostic::spanned(
                self.1.span(),
                DiagLevel::Error,
                format!("`min_length` error: {}", error),
            )
            .help(
                "See more details: `http://json-schema.org/draft/2020-12/json-schema-validation.html#name-minlength`",
            ))
        } else {
            Ok(())
        }
    }
}
impl Parse for MinLength {
    fn parse(input: ParseStream, ident: Ident) -> syn::Result<Self>
    where
        Self: Sized,
    {
        parse_integer(input).map(|max_length| Self(max_length, ident))
    }
}
impl ToTokens for MinLength {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<MinLength> for Feature {
    fn from(value: MinLength) -> Self {
        Feature::MinLength(value)
    }
}
impl_get_name!(MinLength = "min_length");

#[derive(Clone, Debug)]
pub(crate) struct Pattern(pub(crate) String, pub(crate) Ident);
impl Validate for Pattern {
    fn validate(&self, validator: impl Validator) -> DiagResult<()> {
        if let Err(error) = validator.is_valid() {
            Err(
                Diagnostic::spanned(self.1.span(), DiagLevel::Error, format!("`pattern` error: {}", error)).help(
                    "See more details: `http://json-schema.org/draft/2020-12/json-schema-validation.html#name-pattern`",
                ),
            )
        } else {
            Ok(())
        }
    }
}
impl Parse for Pattern {
    fn parse(input: ParseStream, ident: Ident) -> syn::Result<Self>
    where
        Self: Sized,
    {
        parse_utils::parse_next(input, || input.parse::<LitStr>()).map(|pattern| Self(pattern.value(), ident))
    }
}
impl ToTokens for Pattern {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<Pattern> for Feature {
    fn from(value: Pattern) -> Self {
        Feature::Pattern(value)
    }
}
impl_get_name!(Pattern = "pattern");

#[derive(Clone, Debug)]
pub(crate) struct MaxItems(pub(crate) usize, pub(crate) Ident);
impl Validate for MaxItems {
    fn validate(&self, validator: impl Validator) -> DiagResult<()> {
        if let Err(error) = validator.is_valid() {
            Err(
                Diagnostic::spanned(self.1.span(), DiagLevel::Error, format!("`max_items` error: {}", error)).help(
                    "See more details: `http://json-schema.org/draft/2020-12/json-schema-validation.html#name-maxitems",
                ),
            )
        } else {
            Ok(())
        }
    }
}
impl Parse for MaxItems {
    fn parse(input: ParseStream, ident: Ident) -> syn::Result<Self>
    where
        Self: Sized,
    {
        parse_number(input).map(|max_items| Self(max_items, ident))
    }
}
impl ToTokens for MaxItems {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<MaxItems> for Feature {
    fn from(value: MaxItems) -> Self {
        Feature::MaxItems(value)
    }
}
impl_get_name!(MaxItems = "max_items");

#[derive(Clone, Debug)]
pub(crate) struct MinItems(pub(crate) usize, pub(crate) Ident);
impl Validate for MinItems {
    fn validate(&self, validator: impl Validator) -> DiagResult<()> {
        if let Err(error) = validator.is_valid() {
            Err(
                Diagnostic::spanned(self.1.span(), DiagLevel::Error, format!("`min_items` error: {}", error)).help(
                    "See more details: `http://json-schema.org/draft/2020-12/json-schema-validation.html#name-minitems",
                ),
            )
        } else {
            Ok(())
        }
    }
}
impl Parse for MinItems {
    fn parse(input: ParseStream, ident: Ident) -> syn::Result<Self>
    where
        Self: Sized,
    {
        parse_number(input).map(|max_items| Self(max_items, ident))
    }
}
impl ToTokens for MinItems {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<MinItems> for Feature {
    fn from(value: MinItems) -> Self {
        Feature::MinItems(value)
    }
}
impl_get_name!(MinItems = "min_items");

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct MaxProperties(pub(crate) usize, pub(crate) Ident);
impl Parse for MaxProperties {
    fn parse(input: ParseStream, ident: Ident) -> syn::Result<Self>
    where
        Self: Sized,
    {
        parse_integer(input).map(|max_properties| Self(max_properties, ident))
    }
}
impl ToTokens for MaxProperties {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<MaxProperties> for Feature {
    fn from(value: MaxProperties) -> Self {
        Feature::MaxProperties(value)
    }
}
impl_get_name!(MaxProperties = "max_properties");

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct MinProperties(pub(crate) usize, pub(crate) Ident);
impl Parse for MinProperties {
    fn parse(input: ParseStream, ident: Ident) -> syn::Result<Self>
    where
        Self: Sized,
    {
        parse_integer(input).map(|min_properties| Self(min_properties, ident))
    }
}
impl ToTokens for MinProperties {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<MinProperties> for Feature {
    fn from(value: MinProperties) -> Self {
        Feature::MinProperties(value)
    }
}
impl_get_name!(MinProperties = "min_properties");

#[derive(Clone, Debug)]
pub(crate) struct SchemaWith(pub(crate) TypePath);
impl Parse for SchemaWith {
    fn parse(input: ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_next(input, || input.parse::<TypePath>().map(Self))
    }
}
impl ToTokens for SchemaWith {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        let path = &self.0;
        stream.extend(quote! {
            #path()
        })
    }
}
impl From<SchemaWith> for Feature {
    fn from(value: SchemaWith) -> Self {
        Feature::SchemaWith(value)
    }
}
impl_get_name!(SchemaWith = "schema_with");

#[derive(Clone, Debug)]
pub(crate) struct Bound(pub(crate) Vec<WherePredicate>);
impl Parse for Bound {
    fn parse(input: ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_next(input, || {
            let input: LitStr = input.parse()?;
            input
                .parse_with(Punctuated::<WherePredicate, token::Comma>::parse_terminated)
                .map(|p| Self(p.into_iter().collect()))
        })
    }
}
impl TryToTokens for Bound {
    fn try_to_tokens(&self, _stream: &mut TokenStream) -> DiagResult<()> {
        Ok(())
    }
}
impl From<Bound> for Feature {
    fn from(value: Bound) -> Self {
        Feature::Bound(value)
    }
}
impl_get_name!(Bound = "bound");

#[derive(Eq, PartialEq, Clone, Debug)]
pub(crate) struct SkipBound(pub(crate) bool);
impl Parse for SkipBound {
    fn parse(input: ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_bool_or_true(input).map(Self)
    }
}
impl ToTokens for SkipBound {
    fn to_tokens(&self, _stream: &mut proc_macro2::TokenStream) {}
}
impl From<SkipBound> for Feature {
    fn from(value: SkipBound) -> Self {
        Feature::SkipBound(value)
    }
}
impl_get_name!(SkipBound = "skip_bound");

#[derive(Clone, Debug)]
pub(crate) struct Description(pub(crate) parse_utils::Value);
impl Parse for Description {
    fn parse(input: ParseStream, _: Ident) -> syn::Result<Self>
    where
        Self: std::marker::Sized,
    {
        parse_utils::parse_next_lit_str_or_expr(input).map(Self)
    }
}
impl ToTokens for Description {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream);
    }
}
impl From<String> for Description {
    fn from(value: String) -> Self {
        Self(value.into())
    }
}
impl From<Description> for Feature {
    fn from(value: Description) -> Self {
        Self::Description(value)
    }
}
impl_get_name!(Description = "description");

/// Deprecated feature parsed from macro attributes.
///
/// This feature supports only syntax parsed from salvo_oapi specific macro attributes, it does not
/// support Rust `#[deprecated]` attribute.
#[derive(Clone, Debug)]
pub(crate) struct Deprecated(pub(crate) bool);
impl Parse for Deprecated {
    fn parse(input: ParseStream, _: Ident) -> syn::Result<Self>
    where
        Self: std::marker::Sized,
    {
        parse_utils::parse_bool_or_true(input).map(Self)
    }
}
impl ToTokens for Deprecated {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        let deprecated: crate::Deprecated = self.0.into();
        deprecated.to_tokens(stream);
    }
}
impl From<Deprecated> for Feature {
    fn from(value: Deprecated) -> Self {
        Self::Deprecated(value)
    }
}

impl_get_name!(Deprecated = "deprecated");

/// Skip feature parsed from macro attributes.
#[derive(Clone, Debug)]
pub(crate) struct Skip(pub(crate) bool);
impl Parse for Skip {
    fn parse(input: ParseStream, _: Ident) -> syn::Result<Self>
    where
        Self: std::marker::Sized,
    {
        parse_utils::parse_bool_or_true(input).map(Self)
    }
}
impl From<bool> for Skip {
    fn from(value: bool) -> Self {
        Skip(value)
    }
}
impl From<Skip> for Feature {
    fn from(value: Skip) -> Self {
        Self::Skip(value)
    }
}

impl_get_name!(Skip = "skip");

#[derive(Clone, Debug)]
pub(crate) struct AdditionalProperties(pub(crate) bool);
impl Parse for AdditionalProperties {
    fn parse(input: ParseStream, _: Ident) -> syn::Result<Self>
    where
        Self: std::marker::Sized,
    {
        parse_utils::parse_bool_or_true(input).map(Self)
    }
}
impl ToTokens for AdditionalProperties {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        let oapi = crate::oapi_crate();
        let additional_properties = &self.0;
        stream.extend(quote!(
            #oapi::oapi::schema::AdditionalProperties::FreeForm(
                #additional_properties
            )
        ))
    }
}
impl_get_name!(AdditionalProperties = "additional_properties");

impl From<AdditionalProperties> for Feature {
    fn from(value: AdditionalProperties) -> Self {
        Self::AdditionalProperties(value)
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Required(pub(crate) bool);
impl Required {
    pub(crate) fn is_true(&self) -> bool {
        self.0
    }
}
impl Parse for Required {
    fn parse(input: ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_bool_or_true(input).map(Self)
    }
}
impl ToTokens for Required {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(stream)
    }
}
impl From<crate::Required> for Required {
    fn from(value: crate::Required) -> Self {
        if value == crate::Required::True {
            Self(true)
        } else {
            Self(false)
        }
    }
}
impl From<bool> for Required {
    fn from(value: bool) -> Self {
        Self(value)
    }
}
impl From<Required> for Feature {
    fn from(value: Required) -> Self {
        Self::Required(value)
    }
}
impl_get_name!(Required = "required");

#[derive(Clone, Debug)]
pub(crate) struct Alias {
    pub(crate) name: String,
    pub(crate) ty: Type,
}

// impl Alias {
//     pub(crate) fn get_lifetimes(&self) -> Result<impl Iterator<Item = &GenericArgument>, Diagnostic> {
//         fn lifetimes_from_type(ty: &Type) -> Result<impl Iterator<Item = &GenericArgument>, Diagnostic> {
//             match ty {
//                 Type::Path(type_path) => Ok(type_path
//                     .path
//                     .segments
//                     .iter()
//                     .flat_map(|segment| match &segment.arguments {
//                         PathArguments::AngleBracketed(angle_bracketed_args) => {
//                             Some(angle_bracketed_args.args.iter())
//                         }
//                         _ => None,
//                     })
//                     .flatten()
//                     .flat_map(|arg| match arg {
//                         GenericArgument::Type(type_argument) => {
//                             lifetimes_from_type(type_argument).map(|iter| iter.collect::<Vec<_>>())
//                         }
//                         _ => Ok(vec![arg]),
//                     })
//                     .flat_map(|args| args.into_iter().filter(|generic_arg| matches!(generic_arg, syn::GenericArgument::Lifetime(lifetime) if lifetime.ident != "'static"))),
//                     ),
//                 _ => Err(Diagnostic::spanned(ty.span(),DiagLevel::Error, "AliasSchema `get_lifetimes` only supports syn::TypePath types"))
//             }
//         }

//         lifetimes_from_type(&self.ty)
//     }
// }

impl syn::parse::Parse for Alias {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name = input.parse::<Ident>()?;
        input.parse::<Token![=]>()?;

        Ok(Self {
            name: name.to_string(),
            ty: input.parse::<Type>()?,
        })
    }
}

// pub(super) fn parse_aliases(attributes: &[Attribute]) -> DiagResult<Option<Punctuated<Alias, Comma>>> {
//     attributes
//         .iter()
//         .find(|attribute| attribute.path().is_ident("aliases"))
//         .map(|aliases| aliases.parse_args_with(Punctuated::<Alias, Comma>::parse_terminated))
//         .transpose()
//         .map_err(Into::into)
// }

#[derive(Default, Clone, Debug)]
pub(crate) struct Aliases(pub(crate) Punctuated<Alias, Comma>);

impl Parse for Aliases {
    fn parse(input: syn::parse::ParseStream, _: Ident) -> syn::Result<Self> {
        parse_utils::parse_punctuated_within_parenthesis(input).map(Self)
    }
}

// impl ToTokens for Aliases {
//     fn to_tokens(&self, stream: &mut TokenStream) {
//         stream.extend(self.0.to_token_stream())
//     }
// }

impl From<Aliases> for Feature {
    fn from(value: Aliases) -> Self {
        Feature::Aliases(value)
    }
}
impl_get_name!(Aliases = "aliases");
