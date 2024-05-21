use std::borrow::Cow;

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::parse::Parse;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::{punctuated::Punctuated, Attribute, Fields, GenericArgument, Ident, PathArguments, Token, Type, Variant};

use crate::{
    doc_comment::CommentAttributes,
    feature::{
        parse_features, pop_feature, pop_feature_as_inner, Bound, Example, Feature, FeaturesExt, IntoInner, IsSkipped,
        Name, Rename, RenameAll, SkipBound, TryToTokensExt,
    },
    schema::{Inline, VariantRename},
    serde_util::{self, SerdeContainer, SerdeEnumRepr, SerdeValue},
    type_tree::{TypeTree, ValueType},
};
use crate::{DiagLevel, DiagResult, Diagnostic, TryToTokens};

use super::{
    enum_variant::{
        self, AdjacentlyTaggedEnum, CustomEnum, Enum, ObjectVariant, SimpleEnumVariant, TaggedEnum, UntaggedEnum,
    },
    feature::{
        self, ComplexEnumFeatures, EnumFeatures, EnumNamedFieldVariantFeatures, EnumUnnamedFieldVariantFeatures,
        FromAttributes,
    },
    is_not_skipped, NamedStructSchema, SchemaFeatureExt, UnnamedStructSchema,
};

#[derive(Debug)]
pub(crate) struct AliasSchema {
    pub(crate) name: String,
    pub(crate) ty: Type,
}

impl AliasSchema {
    pub(crate) fn get_lifetimes(&self) -> Result<impl Iterator<Item = &GenericArgument>, Diagnostic> {
        fn lifetimes_from_type(ty: &Type) -> Result<impl Iterator<Item = &GenericArgument>, Diagnostic> {
            match ty {
                Type::Path(type_path) => Ok(type_path
                    .path
                    .segments
                    .iter()
                    .flat_map(|segment| match &segment.arguments {
                        PathArguments::AngleBracketed(angle_bracketed_args) => {
                            Some(angle_bracketed_args.args.iter())
                        }
                        _ => None,
                    })
                    .flatten()
                    .flat_map(|arg| match arg {
                        GenericArgument::Type(type_argument) => {
                            lifetimes_from_type(type_argument).map(|iter| iter.collect::<Vec<_>>())
                        }
                        _ => Ok(vec![arg]),
                    })
                    .flat_map(|args| args.into_iter().filter(|generic_arg| matches!(generic_arg, syn::GenericArgument::Lifetime(lifetime) if lifetime.ident != "'static"))),
                    ),
                _ => Err(Diagnostic::spanned(ty.span(),DiagLevel::Error, "AliasSchema `get_lifetimes` only supports syn::TypePath types"))
            }
        }

        lifetimes_from_type(&self.ty)
    }
}

impl Parse for AliasSchema {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name = input.parse::<Ident>()?;
        input.parse::<Token![=]>()?;

        Ok(Self {
            name: name.to_string(),
            ty: input.parse::<Type>()?,
        })
    }
}

pub(super) fn parse_aliases(attributes: &[Attribute]) -> DiagResult<Option<Punctuated<AliasSchema, Comma>>> {
    attributes
        .iter()
        .find(|attribute| attribute.path().is_ident("aliases"))
        .map(|aliases| aliases.parse_args_with(Punctuated::<AliasSchema, Comma>::parse_terminated))
        .transpose()
        .map_err(Into::into)
}
