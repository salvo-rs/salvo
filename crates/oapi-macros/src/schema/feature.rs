use syn::parse::{Parse, ParseBuffer, ParseStream};
use syn::Attribute;

use crate::feature::{
    impl_into_inner, impl_merge, parse_features, AdditionalProperties, Default, Deprecated, Example, ExclusiveMaximum,
    ExclusiveMinimum, Feature, Format, Inline, IntoInner, MaxItems, MaxLength, MaxProperties, Maximum, Merge, MinItems,
    MinLength, MinProperties, Minimum, MultipleOf, Nullable, Pattern, ReadOnly, Rename, RenameAll, Required,
    SchemaWith, Symbol, ValueType, WriteOnly, XmlAttr,
};
use crate::{attribute, ResultExt};

#[derive(Debug)]
pub(crate) struct NamedFieldStructFeatures(Vec<Feature>);

impl Parse for NamedFieldStructFeatures {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(NamedFieldStructFeatures(parse_features!(
            input as Example,
            XmlAttr,
            Symbol,
            RenameAll,
            MaxProperties,
            MinProperties,
            Inline,
            Default,
            Deprecated
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
            Default,
            Symbol,
            Format,
            ValueType,
            Inline,
            Deprecated
        )))
    }
}

impl_into_inner!(UnnamedFieldStructFeatures);

pub(crate) struct EnumFeatures(Vec<Feature>);

impl Parse for EnumFeatures {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(EnumFeatures(parse_features!(
            input as Example,
            Default,
            Symbol,
            RenameAll,
            Inline,
            Deprecated
        )))
    }
}

impl_into_inner!(EnumFeatures);

pub(crate) struct ComplexEnumFeatures(Vec<Feature>);

impl Parse for ComplexEnumFeatures {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(ComplexEnumFeatures(parse_features!(
            input as Example,
            Default,
            RenameAll,
            Symbol,
            Inline,
            Deprecated
        )))
    }
}

impl_into_inner!(ComplexEnumFeatures);

pub(crate) struct NamedFieldFeatures(Vec<Feature>);

impl Parse for NamedFieldFeatures {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(NamedFieldFeatures(parse_features!(
            input as Example,
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
            Deprecated
        )))
    }
}

impl_into_inner!(NamedFieldFeatures);

pub(crate) struct EnumNamedFieldVariantFeatures(Vec<Feature>);

impl Parse for EnumNamedFieldVariantFeatures {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(EnumNamedFieldVariantFeatures(parse_features!(
            input as Example,
            XmlAttr,
            Symbol,
            Rename,
            RenameAll,
            Deprecated
        )))
    }
}

impl_into_inner!(EnumNamedFieldVariantFeatures);

pub(crate) struct EnumUnnamedFieldVariantFeatures(Vec<Feature>);

impl Parse for EnumUnnamedFieldVariantFeatures {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(EnumUnnamedFieldVariantFeatures(parse_features!(
            input as Example,
            Default,
            Symbol,
            Format,
            ValueType,
            Rename,
            Deprecated
        )))
    }
}

impl_into_inner!(EnumUnnamedFieldVariantFeatures);

pub(crate) trait FromAttributes {
    fn parse_features<T>(&self) -> Option<T>
    where
        T: Parse + Merge<T>;
}

impl FromAttributes for &'_ [Attribute] {
    fn parse_features<T>(&self) -> Option<T>
    where
        T: Parse + Merge<T>,
    {
        parse_schema_features::<T>(self)
    }
}

impl FromAttributes for Vec<Attribute> {
    fn parse_features<T>(&self) -> Option<T>
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

pub(crate) fn parse_schema_features<T: Sized + Parse + Merge<T>>(attributes: &[Attribute]) -> Option<T> {
    attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("salvo"))
        .filter_map(|attr| attribute::find_nested_list(attr, "schema").ok().flatten())
        .map(|attr| attr.parse_args::<T>().unwrap_or_abort())
        .reduce(|acc, item| acc.merge(item))
}

pub(crate) fn parse_schema_features_with<
    T: Merge<T>,
    P: for<'r> FnOnce(&'r ParseBuffer<'r>) -> syn::Result<T> + Copy,
>(
    attributes: &[Attribute],
    parser: P,
) -> Option<T> {
    attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("schema"))
        .map(|attributes| attributes.parse_args_with(parser).unwrap_or_abort())
        .reduce(|acc, item| acc.merge(item))
}
