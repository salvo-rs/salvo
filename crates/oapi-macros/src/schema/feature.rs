use syn::Attribute;
use syn::parse::{Parse, ParseBuffer, ParseStream};

use crate::feature::attributes::{
    AdditionalProperties, Aliases, Bound, ContentEncoding, ContentMediaType, Default, Deprecated,
    Description, Example, Examples, Format, Inline, Name, Nullable, ReadOnly, Rename, RenameAll,
    Required, SchemaWith, Skip, SkipBound, Title, ValueType, WriteOnly, XmlAttr,
};
use crate::feature::validation::{
    ExclusiveMaximum, ExclusiveMinimum, MaxItems, MaxLength, MaxProperties, Maximum, MinItems,
    MinLength, MinProperties, Minimum, MultipleOf, Pattern,
};
use crate::feature::{Feature, Merge, impl_into_inner, impl_merge, parse_features};
use crate::{DiagResult, Diagnostic, IntoInner, attribute};

#[derive(Debug)]
pub(crate) struct NamedFieldStructFeatures(Vec<Feature>);

impl Parse for NamedFieldStructFeatures {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(NamedFieldStructFeatures(parse_features!(
            input as Example,
            Examples,
            XmlAttr,
            Name,
            Title,
            Aliases,
            RenameAll,
            MaxProperties,
            MinProperties,
            Inline,
            Default,
            Deprecated,
            Description,
            Skip,
            Bound,
            SkipBound
        )))
    }
}

impl_into_inner!(NamedFieldStructFeatures);

#[derive(Debug)]
pub(crate) struct UnnamedFieldStructFeatures(Vec<Feature>);

impl Parse for UnnamedFieldStructFeatures {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(UnnamedFieldStructFeatures(parse_features!(
            input as Example,
            Examples,
            Default,
            Name,
            Title,
            Aliases,
            Format,
            ValueType,
            Inline,
            Deprecated,
            Description,
            Skip,
            Bound,
            SkipBound
        )))
    }
}

impl_into_inner!(UnnamedFieldStructFeatures);

pub(crate) struct EnumFeatures(Vec<Feature>);

impl Parse for EnumFeatures {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(EnumFeatures(parse_features!(
            input as Example,
            Examples,
            Default,
            Name,
            Title,
            Aliases,
            RenameAll,
            Inline,
            Deprecated,
            Description,
            Bound,
            SkipBound
        )))
    }
}

impl_into_inner!(EnumFeatures);

pub(crate) struct ComplexEnumFeatures(Vec<Feature>);

impl Parse for ComplexEnumFeatures {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(ComplexEnumFeatures(parse_features!(
            input as Example,
            Examples,
            Default,
            RenameAll,
            Name,
            Title,
            Aliases,
            Inline,
            Deprecated,
            Description,
            Bound,
            SkipBound,
        )))
    }
}

impl_into_inner!(ComplexEnumFeatures);

pub(crate) struct NamedFieldFeatures(Vec<Feature>);

impl Parse for NamedFieldFeatures {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(NamedFieldFeatures(parse_features!(
            input as Example,
            Examples,
            ValueType,
            Format,
            Default,
            WriteOnly,
            ReadOnly,
            XmlAttr,
            Inline,
            Nullable,
            Rename,
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
            SchemaWith,
            AdditionalProperties,
            Required,
            Deprecated,
            Skip,
            ContentEncoding,
            ContentMediaType
        )))
    }
}

impl_into_inner!(NamedFieldFeatures);

pub(crate) struct EnumNamedFieldVariantFeatures(Vec<Feature>);

impl Parse for EnumNamedFieldVariantFeatures {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(EnumNamedFieldVariantFeatures(parse_features!(
            input as Example,
            Examples,
            XmlAttr,
            Title,
            Rename,
            RenameAll,
            Deprecated,
            Skip
        )))
    }
}

impl_into_inner!(EnumNamedFieldVariantFeatures);

pub(crate) struct EnumUnnamedFieldVariantFeatures(Vec<Feature>);

impl Parse for EnumUnnamedFieldVariantFeatures {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(EnumUnnamedFieldVariantFeatures(parse_features!(
            input as Example,
            Examples,
            Default,
            Title,
            Format,
            ValueType,
            Rename,
            Deprecated,
            Skip
        )))
    }
}

impl_into_inner!(EnumUnnamedFieldVariantFeatures);

pub(crate) trait FromAttributes {
    fn parse_features<T>(&self) -> Result<Option<T>, Diagnostic>
    where
        T: Parse + Merge<T>;
}

impl FromAttributes for &'_ [Attribute] {
    fn parse_features<T>(&self) -> Result<Option<T>, Diagnostic>
    where
        T: Parse + Merge<T>,
    {
        parse_schema_features::<T>(self)
    }
}

impl FromAttributes for Vec<Attribute> {
    fn parse_features<T>(&self) -> Result<Option<T>, Diagnostic>
    where
        T: Parse + Merge<T>,
    {
        parse_schema_features::<T>(self)
    }
}

impl_merge!(
    NamedFieldStructFeatures,
    UnnamedFieldStructFeatures,
    EnumFeatures,
    ComplexEnumFeatures,
    NamedFieldFeatures,
    EnumNamedFieldVariantFeatures,
    EnumUnnamedFieldVariantFeatures
);

pub(crate) fn parse_schema_features<T: Sized + Parse + Merge<T>>(
    attributes: &[Attribute],
) -> DiagResult<Option<T>> {
    Ok(attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("salvo"))
        .filter_map(|attr| attribute::find_nested_list(attr, "schema").ok().flatten())
        .map(|attr| attr.parse_args::<T>().map_err(Diagnostic::from))
        .collect::<Result<Vec<T>, Diagnostic>>()?
        .into_iter()
        .reduce(|acc, item| acc.merge(item)))
}

pub(crate) fn parse_schema_features_with<
    T: Merge<T>,
    P: for<'r> FnOnce(&'r ParseBuffer<'r>) -> syn::Result<T> + Copy,
>(
    attributes: &[Attribute],
    parser: P,
) -> DiagResult<Option<T>> {
    Ok(attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("schema"))
        .map(|attributes| attributes.parse_args_with(parser).map_err(Diagnostic::from))
        .collect::<Result<Vec<T>, Diagnostic>>()?
        .into_iter()
        .reduce(|acc, item| acc.merge(item)))
}
