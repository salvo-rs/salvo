use std::fmt::Display;

use proc_macro2::{Ident, Span, TokenStream};
use quote::ToTokens;
use syn::LitStr;
use syn::parse::ParseStream;

use super::{Feature, Parse, Validate, Validator, impl_get_name, parse_integer, parse_number};
use crate::{DiagLevel, DiagResult, Diagnostic, parse_utils};

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
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
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
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
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
    fn to_tokens(&self, stream: &mut TokenStream) {
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
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
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
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
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
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
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
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
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
        parse_utils::parse_next(input, || input.parse::<LitStr>())
            .map(|pattern| Self(pattern.value(), ident))
    }
}
impl ToTokens for Pattern {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
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
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
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
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
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
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
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
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
    }
}
impl From<MinProperties> for Feature {
    fn from(value: MinProperties) -> Self {
        Feature::MinProperties(value)
    }
}
impl_get_name!(MinProperties = "min_properties");
