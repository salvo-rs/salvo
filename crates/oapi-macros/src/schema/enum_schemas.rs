use std::borrow::Cow;

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::{Attribute, Fields, Generics, Token, Variant};

use crate::component::ComponentDescription;
use crate::doc_comment::CommentAttributes;
use crate::feature::attributes::{
    Alias, Bound, Example, Name, Rename, RenameAll, SkipBound, Title,
};
use crate::feature::{
    Feature, FeaturesExt, IsSkipped, TryToTokensExt, parse_features, pop_feature,
    pop_feature_as_inner,
};
use crate::schema::{Description, Inline, VariantRename};
use crate::serde_util::{self, SerdeContainer, SerdeEnumRepr, SerdeValue};
use crate::type_tree::{TypeTree, ValueType};
use crate::{DiagLevel, DiagResult, Diagnostic, IntoInner, TryToTokens};

use super::enum_variant::{
    self, AdjacentlyTaggedEnum, CustomEnum, Enum, ObjectVariant, SimpleEnumVariant, TaggedEnum,
    UntaggedEnum,
};
use super::feature::{
    self, ComplexEnumFeatures, EnumFeatures, EnumNamedFieldVariantFeatures,
    EnumUnnamedFieldVariantFeatures, FromAttributes,
};
use super::{NamedStructSchema, SchemaFeatureExt, UnnamedStructSchema, is_not_skipped};

#[derive(Debug)]
pub(crate) struct EnumSchema<'a> {
    pub(super) schema_type: EnumSchemaType<'a>,
    pub(super) name: Option<Name>,
    pub(super) aliases: Option<Punctuated<Alias, Comma>>,
    #[allow(dead_code)]
    pub(crate) generics: Option<&'a Generics>,
    pub(super) inline: Option<Inline>,
}

impl<'e> EnumSchema<'e> {
    pub(crate) fn new(
        enum_name: Cow<'e, str>,
        variants: &'e Punctuated<Variant, Token![,]>,
        attributes: &'e [Attribute],
        aliases: Option<Punctuated<Alias, Comma>>,
        generics: Option<&'e Generics>,
    ) -> DiagResult<Self> {
        if variants
            .iter()
            .all(|variant| matches!(variant.fields, Fields::Unit))
        {
            #[cfg(feature = "repr")]
            {
                attributes
                    .iter()
                    .find_map(|attribute| {
                        if attribute.path().is_ident("repr") {
                            attribute.parse_args::<syn::TypePath>().ok()
                        } else {
                            None
                        }
                    })
                    .map(|enum_type| {
                        let mut repr_enum_features =
                            feature::parse_schema_features_with(attributes, |input| {
                                Ok(parse_features!(
                                    input as Example,
                                    crate::feature::attributes::Examples,
                                    crate::feature::attributes::Default,
                                    Name,
                                    Title,
                                    crate::feature::attributes::Inline
                                ))
                            })?
                            .unwrap_or_default();

                        let name = pop_feature_as_inner!(repr_enum_features => Feature::Name(_v));

                        let inline: Option<Inline> =
                            pop_feature_as_inner!(repr_enum_features => Feature::Inline(_v));
                        let description =
                            pop_feature!(repr_enum_features => Feature::Description(_))
                                .into_inner();
                        Ok(Self {
                            schema_type: EnumSchemaType::Repr(ReprEnum {
                                variants,
                                attributes,
                                description,
                                enum_type,
                                enum_features: repr_enum_features,
                            }),
                            name,
                            aliases: aliases.clone(),
                            inline,
                            generics,
                        })
                    })
                    .unwrap_or_else(|| {
                        let mut simple_enum_features = attributes
                            .parse_features::<EnumFeatures>()?
                            .into_inner()
                            .unwrap_or_default();

                        let name = pop_feature_as_inner!(simple_enum_features => Feature::Name(_v));

                        let rename_all = simple_enum_features.pop_rename_all_feature();
                        let inline: Option<Inline> =
                            pop_feature_as_inner!(simple_enum_features => Feature::Inline(_v));
                        let description =
                            pop_feature!(simple_enum_features => Feature::Description(_))
                                .into_inner();
                        Ok(Self {
                            schema_type: EnumSchemaType::Simple(SimpleEnum {
                                attributes,
                                description,
                                variants,
                                enum_features: simple_enum_features,
                                rename_all,
                            }),
                            name,
                            aliases,
                            inline,
                            generics,
                        })
                    })
            }

            #[cfg(not(feature = "repr"))]
            {
                let mut simple_enum_features = attributes
                    .parse_features::<EnumFeatures>()?
                    .into_inner()
                    .unwrap_or_default();

                let generic_count = generics
                    .map(|g| g.type_params().count())
                    .unwrap_or_default();
                let name = pop_feature_as_inner!(simple_enum_features => Feature::Name(_v));
                if generic_count == 0 && !aliases.as_ref().map(|a| a.is_empty()).unwrap_or(true) {
                    return Err(Diagnostic::new(
                        DiagLevel::Error,
                        "aliases are only allowed for generic types",
                    ));
                }

                let rename_all = simple_enum_features.pop_rename_all_feature();
                let inline: Option<Inline> =
                    pop_feature_as_inner!(simple_enum_features => Feature::Inline(_v));
                let description =
                    pop_feature!(simple_enum_features => Feature::Description(_)).into_inner();
                Ok(Self {
                    schema_type: EnumSchemaType::Simple(SimpleEnum {
                        attributes,
                        description,
                        variants,
                        enum_features: simple_enum_features,
                        rename_all,
                    }),
                    name,
                    aliases,
                    inline,
                    generics,
                })
            }
        } else {
            let mut enum_features = attributes
                .parse_features::<ComplexEnumFeatures>()?
                .into_inner()
                .unwrap_or_default();

            let generic_count = generics
                .map(|g| g.type_params().count())
                .unwrap_or_default();
            let name = pop_feature_as_inner!(enum_features => Feature::Name(_v));
            if generic_count == 0 && !aliases.as_ref().map(|a| a.is_empty()).unwrap_or(true) {
                return Err(Diagnostic::new(
                    DiagLevel::Error,
                    "aliases are only allowed for generic types",
                ));
            }

            let rename_all = enum_features.pop_rename_all_feature();
            let inline: Option<Inline> =
                pop_feature_as_inner!(enum_features => Feature::Inline(_v));
            let description = pop_feature!(enum_features => Feature::Description(_)).into_inner();
            Ok(Self {
                schema_type: EnumSchemaType::Complex(ComplexEnum {
                    enum_name,
                    attributes,
                    description,
                    variants,
                    rename_all,
                    enum_features,
                }),
                name,
                aliases,
                inline,
                generics,
            })
        }
    }
    pub(crate) fn pop_skip_bound(&mut self) -> Option<SkipBound> {
        self.schema_type.pop_skip_bound()
    }
    pub(crate) fn pop_bound(&mut self) -> Option<Bound> {
        self.schema_type.pop_bound()
    }
}

impl TryToTokens for EnumSchema<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        self.schema_type.try_to_tokens(tokens)
    }
}

#[derive(Debug)]
pub(super) enum EnumSchemaType<'e> {
    Simple(SimpleEnum<'e>),
    #[cfg(feature = "repr")]
    Repr(ReprEnum<'e>),
    Complex(ComplexEnum<'e>),
}
impl EnumSchemaType<'_> {
    pub(crate) fn pop_skip_bound(&mut self) -> Option<SkipBound> {
        match self {
            Self::Simple(simple) => simple.pop_skip_bound(),
            #[cfg(feature = "repr")]
            Self::Repr(repr) => repr.pop_skip_bound(),
            Self::Complex(complex) => complex.pop_skip_bound(),
        }
    }
    pub(crate) fn pop_bound(&mut self) -> Option<Bound> {
        match self {
            Self::Simple(simple) => simple.pop_bound(),
            #[cfg(feature = "repr")]
            Self::Repr(repr) => repr.pop_bound(),
            Self::Complex(complex) => complex.pop_bound(),
        }
    }
}

impl TryToTokens for EnumSchemaType<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let (attributes, description) = match self {
            Self::Simple(simple) => {
                simple.try_to_tokens(tokens)?;
                (simple.attributes, &simple.description)
            }
            #[cfg(feature = "repr")]
            Self::Repr(repr) => {
                repr.try_to_tokens(tokens)?;
                (repr.attributes, &repr.description)
            }
            Self::Complex(complex) => {
                complex.try_to_tokens(tokens)?;
                (complex.attributes, &complex.description)
            }
        };

        if let Some(deprecated) = crate::get_deprecated(attributes) {
            tokens.extend(quote! { .deprecated(#deprecated) });
        }

        let comments = CommentAttributes::from_attributes(attributes);
        let description = description
            .as_ref()
            .map(ComponentDescription::Description)
            .or(Some(ComponentDescription::CommentAttributes(&comments)));

        description.to_tokens(tokens);
        Ok(())
    }
}

#[cfg(feature = "repr")]
#[derive(Debug)]
pub(super) struct ReprEnum<'a> {
    variants: &'a Punctuated<Variant, Token![,]>,
    attributes: &'a [Attribute],
    description: Option<Description>,
    enum_type: syn::TypePath,
    enum_features: Vec<Feature>,
}
#[cfg(feature = "repr")]
impl ReprEnum<'_> {
    pub(crate) fn pop_skip_bound(&mut self) -> Option<SkipBound> {
        pop_feature_as_inner!(self.enum_features => Feature::SkipBound(_v))
    }
    pub(crate) fn pop_bound(&mut self) -> Option<Bound> {
        pop_feature_as_inner!(self.enum_features => Feature::Bound(_v))
    }
}

#[cfg(feature = "repr")]
impl TryToTokens for ReprEnum<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let container_rules = serde_util::parse_container(self.attributes);

        regular_enum_to_tokens(tokens, &container_rules, &self.enum_features, || {
            self.variants
                .iter()
                .filter_map(|variant| {
                    let variant_type = &variant.ident;
                    let variant_rules = serde_util::parse_value(&variant.attrs);

                    if is_not_skipped(variant_rules.as_ref()) {
                        let repr_type = &self.enum_type;
                        Some(enum_variant::ReprVariant {
                            value: quote! { Self::#variant_type as #repr_type },
                            type_path: repr_type,
                        })
                    } else {
                        None
                    }
                })
                .collect::<Vec<enum_variant::ReprVariant<TokenStream>>>()
        })
    }
}

fn rename_enum_variant<'a>(
    name: &'a str,
    features: &mut Vec<Feature>,
    variant_rules: &'a Option<SerdeValue>,
    container_rules: &'a Option<SerdeContainer>,
    rename_all: &'a Option<RenameAll>,
) -> Option<Cow<'a, str>> {
    let rename = features
        .pop_rename_feature()
        .map(|rename| rename.into_value());
    let rename_to = variant_rules
        .as_ref()
        .and_then(|variant_rules| variant_rules.rename.as_deref().map(Cow::Borrowed))
        .or_else(|| rename.map(Cow::Owned));

    let rename_all = container_rules
        .as_ref()
        .and_then(|container_rules| container_rules.rename_all.as_ref())
        .or_else(|| {
            rename_all
                .as_ref()
                .map(|rename_all| rename_all.as_rename_rule())
        });

    crate::rename::<VariantRename>(name, rename_to, rename_all)
}

#[derive(Debug)]
pub(super) struct SimpleEnum<'a> {
    variants: &'a Punctuated<Variant, Token![,]>,
    description: Option<Description>,
    attributes: &'a [Attribute],
    enum_features: Vec<Feature>,
    rename_all: Option<RenameAll>,
}
impl SimpleEnum<'_> {
    pub(crate) fn pop_skip_bound(&mut self) -> Option<SkipBound> {
        pop_feature_as_inner!(self.enum_features => Feature::SkipBound(_v))
    }
    pub(crate) fn pop_bound(&mut self) -> Option<Bound> {
        pop_feature_as_inner!(self.enum_features => Feature::Bound(_v))
    }
}

impl TryToTokens for SimpleEnum<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let container_rules = serde_util::parse_container(self.attributes);

        regular_enum_to_tokens(tokens, &container_rules, &self.enum_features, || {
            self.variants
                .iter()
                .filter_map(|variant| {
                    let variant_rules = serde_util::parse_value(&variant.attrs);

                    if is_not_skipped(variant_rules.as_ref()) {
                        Some((variant, variant_rules))
                    } else {
                        None
                    }
                })
                .flat_map(|(variant, variant_rules)| {
                    let name = &*variant.ident.to_string();
                    let mut variant_features =
                        feature::parse_schema_features_with(&variant.attrs, |input| {
                            Ok(parse_features!(input as Rename))
                        })
                        .ok()?
                        .unwrap_or_default();
                    let variant_name = rename_enum_variant(
                        name,
                        &mut variant_features,
                        &variant_rules,
                        &container_rules,
                        &self.rename_all,
                    );

                    variant_name
                        .map(|name| SimpleEnumVariant {
                            value: name.to_token_stream(),
                        })
                        .or_else(|| {
                            Some(SimpleEnumVariant {
                                value: name.to_token_stream(),
                            })
                        })
                })
                .collect::<Vec<SimpleEnumVariant<TokenStream>>>()
        })
    }
}

fn regular_enum_to_tokens<T: self::enum_variant::Variant>(
    tokens: &mut TokenStream,
    container_rules: &Option<SerdeContainer>,
    enum_variant_features: &Vec<Feature>,
    get_variants_tokens_vec: impl FnOnce() -> Vec<T>,
) -> DiagResult<()> {
    let enum_values = get_variants_tokens_vec();

    tokens.extend(match container_rules {
        Some(serde_container) => match &serde_container.enum_repr {
            SerdeEnumRepr::ExternallyTagged => Enum::new(enum_values).to_token_stream(),
            SerdeEnumRepr::InternallyTagged { tag } => TaggedEnum::new(
                enum_values
                    .into_iter()
                    .map(|variant| (Cow::Borrowed(tag.as_str()), variant)),
            )
            .to_token_stream(),
            SerdeEnumRepr::Untagged => match UntaggedEnum::new().try_to_token_stream() {
                Ok(tokens) => tokens,
                Err(diag) => diag.emit_as_item_tokens(),
            },
            SerdeEnumRepr::AdjacentlyTagged { tag, content } => {
                AdjacentlyTaggedEnum::new(enum_values.into_iter().map(|variant| {
                    (
                        Cow::Borrowed(tag.as_str()),
                        Cow::Borrowed(content.as_str()),
                        variant,
                    )
                }))
                .to_token_stream()
            }
            // This should not be possible as serde should not let that happen
            SerdeEnumRepr::UnfinishedAdjacentlyTagged { .. } => panic!("Invalid serde enum repr"),
        },
        _ => Enum::new(enum_values).to_token_stream(),
    });

    tokens.extend(enum_variant_features.try_to_token_stream()?);
    Ok(())
}

#[derive(Debug)]
pub(super) struct ComplexEnum<'a> {
    variants: &'a Punctuated<Variant, Token![,]>,
    attributes: &'a [Attribute],
    description: Option<Description>,
    enum_name: Cow<'a, str>,
    enum_features: Vec<Feature>,
    rename_all: Option<RenameAll>,
}

impl ComplexEnum<'_> {
    pub(crate) fn pop_skip_bound(&mut self) -> Option<SkipBound> {
        pop_feature_as_inner!(self.enum_features => Feature::SkipBound(_v))
    }
    pub(crate) fn pop_bound(&mut self) -> Option<Bound> {
        pop_feature_as_inner!(self.enum_features => Feature::Bound(_v))
    }

    /// Produce tokens that represent a variant of a [`ComplexEnum`].
    fn variant_tokens(
        &self,
        name: Cow<'_, str>,
        variant: &Variant,
        variant_rules: &Option<SerdeValue>,
        container_rules: &Option<SerdeContainer>,
        rename_all: &Option<RenameAll>,
    ) -> DiagResult<Option<TokenStream>> {
        // TODO need to be able to split variant.attrs for variant and the struct representation!
        match &variant.fields {
            Fields::Named(named_fields) => {
                let (title_features, mut named_struct_features) = variant
                    .attrs
                    .parse_features::<EnumNamedFieldVariantFeatures>()?
                    .into_inner()
                    .map(|features| features.split_for_title())
                    .unwrap_or_default();

                if named_struct_features.is_skipped() {
                    return Ok(None);
                }

                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut named_struct_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let example = pop_feature!(named_struct_features => Feature::Example(_));

                Ok(Some(self::enum_variant::Variant::to_tokens(
                    &ObjectVariant {
                        name: variant_name.unwrap_or(Cow::Borrowed(&name)),
                        title: title_features
                            .first()
                            .map(TryToTokens::try_to_token_stream)
                            .transpose()?,
                        example: example
                            .as_ref()
                            .map(TryToTokens::try_to_token_stream)
                            .transpose()?,
                        item: NamedStructSchema {
                            struct_name: Cow::Borrowed(&*self.enum_name),
                            attributes: &variant.attrs,
                            description: None,
                            rename_all: named_struct_features.pop_rename_all_feature(),
                            features: Some(named_struct_features),
                            fields: &named_fields.named,
                            generics: None,
                            name: None,
                            aliases: None,
                            inline: None,
                        }
                        .try_to_token_stream()?,
                    },
                )))
            }
            Fields::Unnamed(unnamed_fields) => {
                let (title_features, mut unnamed_struct_features) = variant
                    .attrs
                    .parse_features::<EnumUnnamedFieldVariantFeatures>()?
                    .into_inner()
                    .map(|features| features.split_for_title())
                    .unwrap_or_default();

                if unnamed_struct_features.is_skipped() {
                    return Ok(None);
                }

                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut unnamed_struct_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let example = pop_feature!(unnamed_struct_features => Feature::Example(_));

                Ok(Some(self::enum_variant::Variant::to_tokens(
                    &ObjectVariant {
                        name: variant_name.unwrap_or(Cow::Borrowed(&name)),
                        title: title_features
                            .first()
                            .map(TryToTokens::try_to_token_stream)
                            .transpose()?,
                        example: example
                            .as_ref()
                            .map(TryToTokens::try_to_token_stream)
                            .transpose()?,
                        item: UnnamedStructSchema {
                            struct_name: Cow::Borrowed(&*self.enum_name),
                            attributes: &variant.attrs,
                            description: None,
                            features: Some(unnamed_struct_features),
                            fields: &unnamed_fields.unnamed,
                            name: None,
                            aliases: None,
                            inline: None,
                        }
                        .try_to_token_stream()?,
                    },
                )))
            }
            Fields::Unit => {
                let mut unit_features =
                    feature::parse_schema_features_with(&variant.attrs, |input| {
                        Ok(parse_features!(input as Title, RenameAll, Rename, Example))
                    })?
                    .unwrap_or_default();

                if unit_features.is_skipped() {
                    return Ok(None);
                }

                let title = pop_feature!(unit_features => Feature::Title(_));
                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut unit_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let example = pop_feature!(unit_features => Feature::Example(_));
                let description =
                    CommentAttributes::from_attributes(&variant.attrs).as_formatted_string();
                let description =
                    (!description.is_empty()).then(|| Feature::Description(description.into()));

                // Unit variant is just simple enum with single variant.
                let mut sev = Enum::new([SimpleEnumVariant {
                    value: variant_name
                        .unwrap_or(Cow::Borrowed(&name))
                        .to_token_stream(),
                }]);
                if let Some(title) = title {
                    sev = sev.title(title.try_to_token_stream()?);
                }
                if let Some(example) = example {
                    sev = sev.example(example.try_to_token_stream()?);
                }
                if let Some(description) = description {
                    sev = sev.description(description.try_to_token_stream()?);
                }
                Ok(Some(sev.to_token_stream()))
            }
        }
    }

    /// Produce tokens that represent a variant of a [`ComplexEnum`] where serde enum attribute
    /// `untagged` applies.
    fn untagged_variant_tokens(&self, variant: &Variant) -> DiagResult<Option<TokenStream>> {
        match &variant.fields {
            Fields::Named(named_fields) => {
                let mut named_struct_features = variant
                    .attrs
                    .parse_features::<EnumNamedFieldVariantFeatures>()?
                    .into_inner()
                    .unwrap_or_default();

                if named_struct_features.is_skipped() {
                    return Ok(None);
                }

                NamedStructSchema {
                    struct_name: Cow::Borrowed(&*self.enum_name),
                    attributes: &variant.attrs,
                    description: None,
                    rename_all: named_struct_features.pop_rename_all_feature(),
                    features: Some(named_struct_features),
                    fields: &named_fields.named,
                    generics: None,
                    name: None,
                    aliases: None,
                    inline: None,
                }
                .try_to_token_stream()
                .map(Some)
            }
            Fields::Unnamed(unnamed_fields) => {
                let unnamed_struct_features = variant
                    .attrs
                    .parse_features::<EnumUnnamedFieldVariantFeatures>()?
                    .into_inner()
                    .unwrap_or_default();

                if unnamed_struct_features.is_skipped() {
                    return Ok(None);
                }

                UnnamedStructSchema {
                    struct_name: Cow::Borrowed(&*self.enum_name),
                    attributes: &variant.attrs,
                    description: None,
                    features: Some(unnamed_struct_features),
                    fields: &unnamed_fields.unnamed,
                    name: None,
                    aliases: None,
                    inline: None,
                }
                .try_to_token_stream()
                .map(Some)
            }
            Fields::Unit => {
                let mut unit_features =
                    feature::parse_schema_features_with(&variant.attrs, |input| {
                        Ok(parse_features!(input as Title))
                    })?
                    .unwrap_or_default();

                if unit_features.is_skipped() {
                    return Ok(None);
                }

                let title = pop_feature!(unit_features => Feature::Title(_));

                UntaggedEnum::with_title(title)
                    .try_to_token_stream()
                    .map(Some)
            }
        }
    }

    /// Produce tokens that represent a variant of a [`ComplexEnum`] where serde enum attribute
    /// `tag = ` applies.
    fn tagged_variant_tokens(
        &self,
        tag: &str,
        name: Cow<'_, str>,
        variant: &Variant,
        variant_rules: &Option<SerdeValue>,
        container_rules: &Option<SerdeContainer>,
        rename_all: &Option<RenameAll>,
    ) -> DiagResult<Option<TokenStream>> {
        let oapi = crate::oapi_crate();
        match &variant.fields {
            Fields::Named(named_fields) => {
                let (title_features, mut named_struct_features) = variant
                    .attrs
                    .parse_features::<EnumNamedFieldVariantFeatures>()?
                    .into_inner()
                    .map(|features| features.split_for_title())
                    .unwrap_or_default();

                if named_struct_features.is_skipped() {
                    return Ok(None);
                }

                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut named_struct_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let named_enum = NamedStructSchema {
                    struct_name: Cow::Borrowed(&*self.enum_name),
                    attributes: &variant.attrs,
                    description: None,
                    rename_all: named_struct_features.pop_rename_all_feature(),
                    features: Some(named_struct_features),
                    fields: &named_fields.named,
                    generics: None,
                    name: None,
                    aliases: None,
                    inline: None,
                }
                .try_to_token_stream()?;
                let title = title_features
                    .first()
                    .map(TryToTokens::try_to_token_stream)
                    .transpose()?;

                let variant_name_tokens = Enum::new([SimpleEnumVariant {
                    value: variant_name
                        .unwrap_or(Cow::Borrowed(&name))
                        .to_token_stream(),
                }]);
                Ok(Some(quote! {
                    #named_enum
                        #title
                        .property(#tag, #variant_name_tokens)
                        .required(#tag)
                }))
            }
            Fields::Unnamed(unnamed_fields) => {
                if unnamed_fields.unnamed.len() == 1 {
                    let (title_features, mut unnamed_struct_features) = variant
                        .attrs
                        .parse_features::<EnumUnnamedFieldVariantFeatures>()?
                        .into_inner()
                        .map(|features| features.split_for_title())
                        .unwrap_or_default();

                    if unnamed_struct_features.is_skipped() {
                        return Ok(None);
                    }

                    let variant_name = rename_enum_variant(
                        name.as_ref(),
                        &mut unnamed_struct_features,
                        variant_rules,
                        container_rules,
                        rename_all,
                    );

                    let unnamed_enum = UnnamedStructSchema {
                        struct_name: Cow::Borrowed(&*self.enum_name),
                        attributes: &variant.attrs,
                        description: None,
                        features: Some(unnamed_struct_features),
                        fields: &unnamed_fields.unnamed,
                        name: None,
                        aliases: None,
                        inline: None,
                    }
                    .try_to_token_stream()?;

                    let title = title_features
                        .first()
                        .map(TryToTokens::try_to_token_stream)
                        .transpose()?;
                    let variant_name_tokens = Enum::new([SimpleEnumVariant {
                        value: variant_name
                            .unwrap_or(Cow::Borrowed(&name))
                            .to_token_stream(),
                    }]);

                    let is_reference = unnamed_fields.unnamed.iter().any(|field| {
                        TypeTree::from_type(&field.ty)
                            .map(|ty| ty.value_type == ValueType::Object)
                            .unwrap_or(false)
                    });

                    if is_reference {
                        Ok(Some(quote! {
                            #oapi::oapi::schema::AllOf::new()
                                #title
                                .item(#unnamed_enum)
                                .item(#oapi::oapi::schema::Object::new()
                                    .schema_type(#oapi::oapi::schema::BasicType::Object)
                                    .property(#tag, #variant_name_tokens)
                                    .required(#tag)
                                )
                        }))
                    } else {
                        Ok(Some(quote! {
                            #unnamed_enum
                                #name
                                .schema_type(#oapi::oapi::schema::BasicType::Object)
                                .property(#tag, #variant_name_tokens)
                                .required(#tag)
                        }))
                    }
                } else {
                    Err(Diagnostic::spanned(
                        variant.span(),
                        DiagLevel::Error,
                        "Unnamed (tuple) enum variants are unsupported for internally tagged enums using the `tag = ` serde attribute"
                    ).help("Try using a different serde enum representation").note("See more about enum limitations here: `https://serde.rs/enum-representations.html#internally-tagged`"))
                }
            }
            Fields::Unit => {
                let mut unit_features =
                    feature::parse_schema_features_with(&variant.attrs, |input| {
                        Ok(parse_features!(input as Title, Rename))
                    })?
                    .unwrap_or_default();

                if unit_features.is_skipped() {
                    return Ok(None);
                }

                let title = pop_feature!(unit_features => Feature::Title(_))
                    .map(|f| f.try_to_token_stream())
                    .transpose()?;

                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut unit_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                // Unit variant is just simple enum with single variant.
                let variant_tokens = Enum::new([SimpleEnumVariant {
                    value: variant_name
                        .unwrap_or(Cow::Borrowed(&name))
                        .to_token_stream(),
                }]);

                Ok(Some(quote! {
                    #oapi::oapi::schema::Object::new()
                        #title
                        .property(#tag, #variant_tokens)
                        .required(#tag)
                }))
            }
        }
    }

    // FIXME perhaps design this better to lessen the amount of args.
    #[allow(clippy::too_many_arguments)]
    fn adjacently_tagged_variant_tokens(
        &self,
        tag: &str,
        content: &str,
        name: Cow<'_, str>,
        variant: &Variant,
        variant_rules: &Option<SerdeValue>,
        container_rules: &Option<SerdeContainer>,
        rename_all: &Option<RenameAll>,
    ) -> DiagResult<Option<TokenStream>> {
        let oapi = crate::oapi_crate();
        match &variant.fields {
            Fields::Named(named_fields) => {
                let (title_features, mut named_struct_features) = variant
                    .attrs
                    .parse_features::<EnumNamedFieldVariantFeatures>()?
                    .into_inner()
                    .map(|features| features.split_for_title())
                    .unwrap_or_default();

                if named_struct_features.is_skipped() {
                    return Ok(None);
                }

                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut named_struct_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                let named_enum = NamedStructSchema {
                    struct_name: Cow::Borrowed(&*self.enum_name),
                    attributes: &variant.attrs,
                    description: None,
                    rename_all: named_struct_features.pop_rename_all_feature(),
                    features: Some(named_struct_features),
                    fields: &named_fields.named,
                    generics: None,
                    name: None,
                    aliases: None,
                    inline: None,
                }
                .try_to_token_stream()?;
                let title = title_features
                    .first()
                    .map(|s| s.try_to_token_stream())
                    .transpose()?;

                let variant_name_tokens = Enum::new([SimpleEnumVariant {
                    value: variant_name
                        .unwrap_or(Cow::Borrowed(&name))
                        .to_token_stream(),
                }]);
                Ok(Some(quote! {
                    #oapi::oapi::schema::Object::new()
                        #title
                        .schema_type(#oapi::oapi::schema::BasicType::Object)
                        .property(#tag, #variant_name_tokens)
                        .required(#tag)
                        .property(#content, #named_enum)
                        .required(#content)
                }))
            }
            Fields::Unnamed(unnamed_fields) => {
                if unnamed_fields.unnamed.len() == 1 {
                    let (title_features, mut unnamed_struct_features) = variant
                        .attrs
                        .parse_features::<EnumUnnamedFieldVariantFeatures>()?
                        .into_inner()
                        .map(|features| features.split_for_title())
                        .unwrap_or_default();

                    if unnamed_struct_features.is_skipped() {
                        return Ok(None);
                    }

                    let variant_name = rename_enum_variant(
                        name.as_ref(),
                        &mut unnamed_struct_features,
                        variant_rules,
                        container_rules,
                        rename_all,
                    );

                    let unnamed_enum = UnnamedStructSchema {
                        struct_name: Cow::Borrowed(&*self.enum_name),
                        description: None,
                        attributes: &variant.attrs,
                        features: Some(unnamed_struct_features),
                        fields: &unnamed_fields.unnamed,
                        name: None,
                        aliases: None,
                        inline: None,
                    }
                    .try_to_token_stream()?;

                    let title = title_features
                        .first()
                        .map(TryToTokens::try_to_token_stream)
                        .transpose()?
                        .map(|title| quote! { .title(#title)});
                    let variant_name_tokens = Enum::new([SimpleEnumVariant {
                        value: variant_name
                            .unwrap_or(Cow::Borrowed(&name))
                            .to_token_stream(),
                    }]);

                    Ok(Some(quote! {
                        #oapi::oapi::schema::Object::new()
                            #title
                            .schema_type(#oapi::oapi::schema::BasicType::Object)
                            .property(#tag, #variant_name_tokens)
                            .required(#tag)
                            .property(#content, #unnamed_enum)
                            .required(#content)
                    }))
                } else {
                    Err(Diagnostic::spanned(
                        variant.span(),
                        DiagLevel::Error,
                        "Unnamed (tuple) enum variants are unsupported for adjacently tagged enums using the `tag = <tag>, content = <content>` serde attribute"
                    ).help("Try using a different serde enum representation").note("See more about enum limitations here: `https://serde.rs/enum-representations.html#adjacently-tagged`"))
                }
            }
            Fields::Unit => {
                // In this case `content` is simply ignored - there is nothing to put in it.
                let mut unit_features =
                    feature::parse_schema_features_with(&variant.attrs, |input| {
                        Ok(parse_features!(input as Title, Rename))
                    })?
                    .unwrap_or_default();

                if unit_features.is_skipped() {
                    return Ok(None);
                }

                let title = pop_feature!(unit_features => Feature::Title(_))
                    .map(|f| f.try_to_token_stream())
                    .transpose()?;

                let variant_name = rename_enum_variant(
                    name.as_ref(),
                    &mut unit_features,
                    variant_rules,
                    container_rules,
                    rename_all,
                );

                // Unit variant is just simple enum with single variant.
                let variant_tokens = Enum::new([SimpleEnumVariant {
                    value: variant_name
                        .unwrap_or(Cow::Borrowed(&name))
                        .to_token_stream(),
                }]);

                Ok(Some(quote! {
                    #oapi::oapi::schema::Object::new()
                        #title
                        .property(#tag, #variant_tokens)
                        .required(#tag)
                }))
            }
        }
    }
}

impl TryToTokens for ComplexEnum<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let attributes = &self.attributes;
        let container_rules = serde_util::parse_container(attributes);

        let enum_repr = container_rules
            .as_ref()
            .map(|rules| rules.enum_repr.clone())
            .unwrap_or_default();
        let tag = match &enum_repr {
            SerdeEnumRepr::AdjacentlyTagged { tag, .. }
            | SerdeEnumRepr::InternallyTagged { tag } => Some(tag),
            SerdeEnumRepr::ExternallyTagged
            | SerdeEnumRepr::Untagged
            | SerdeEnumRepr::UnfinishedAdjacentlyTagged { .. } => None,
        };
        let ts = self
            .variants
            .iter()
            .filter_map(|variant: &Variant| {
                let variant_serde_rules = serde_util::parse_value(&variant.attrs);
                if is_not_skipped(variant_serde_rules.as_ref()) {
                    Some((variant, variant_serde_rules))
                } else {
                    None
                }
            })
            .map(|(variant, variant_serde_rules)| {
                let variant_name = &*variant.ident.to_string();

                match &enum_repr {
                    SerdeEnumRepr::ExternallyTagged => self.variant_tokens(
                        Cow::Borrowed(variant_name),
                        variant,
                        &variant_serde_rules,
                        &container_rules,
                        &self.rename_all,
                    ),
                    SerdeEnumRepr::InternallyTagged { tag } => self.tagged_variant_tokens(
                        tag,
                        Cow::Borrowed(variant_name),
                        variant,
                        &variant_serde_rules,
                        &container_rules,
                        &self.rename_all,
                    ),
                    SerdeEnumRepr::Untagged => self.untagged_variant_tokens(variant),
                    SerdeEnumRepr::AdjacentlyTagged { tag, content } => self
                        .adjacently_tagged_variant_tokens(
                            tag,
                            content,
                            Cow::Borrowed(variant_name),
                            variant,
                            &variant_serde_rules,
                            &container_rules,
                            &self.rename_all,
                        ),
                    SerdeEnumRepr::UnfinishedAdjacentlyTagged { .. } => {
                        unreachable!("Serde should not have parsed an UnfinishedAdjacentlyTagged")
                    }
                }
            })
            .collect::<DiagResult<Vec<Option<TokenStream>>>>()?
            .into_iter()
            .flatten()
            .collect::<CustomEnum<'_, TokenStream>>();
        if let Some(tag) = tag {
            ts.discriminator(Cow::Borrowed(tag.as_str()))
                .to_tokens(tokens);
        } else {
            ts.to_tokens(tokens);
        }

        tokens.extend(self.enum_features.try_to_token_stream()?);
        Ok(())
    }
}
